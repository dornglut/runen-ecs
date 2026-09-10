use super::batch::BatchCommands;
use super::queue::CommandQueue;
use crate::bundle::Bundle;
use crate::entity::Entity;
use crate::errors::CommandError;
use crate::world::World;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;

pub struct Commands<'world> {
    queue: CommandQueueStorage,
    _marker: PhantomData<&'world mut World>,
}

enum CommandQueueStorage {
    Owned(CommandQueue),
    ExternalOwner(ExternalCommandQueue),
    ExternalBorrowed(ExternalCommandQueue),
}

#[derive(Clone)]
pub(crate) struct ExternalCommandQueue {
    queue: Rc<RefCell<CommandQueue>>,
    active: Rc<Cell<bool>>,
}

impl ExternalCommandQueue {
    fn new() -> Self {
        Self {
            queue: Rc::new(RefCell::new(Vec::new())),
            active: Rc::new(Cell::new(true)),
        }
    }

    fn assert_active(&self) {
        assert!(
            self.active.get(),
            "commands param escaped its system execution scope"
        );
    }

    fn drain(&self) -> CommandQueue {
        std::mem::take(&mut *self.queue.borrow_mut())
    }
}

impl Commands<'static> {
    pub fn new() -> Self {
        Self {
            queue: CommandQueueStorage::Owned(Vec::new()),
            _marker: PhantomData,
        }
    }

    pub(crate) fn new_external_owner() -> Self {
        Self {
            queue: CommandQueueStorage::ExternalOwner(ExternalCommandQueue::new()),
            _marker: PhantomData,
        }
    }

    pub(crate) fn from_external<'world>(queue: ExternalCommandQueue) -> Commands<'world> {
        Commands {
            queue: CommandQueueStorage::ExternalBorrowed(queue),
            _marker: PhantomData,
        }
    }

    pub(crate) fn external_queue(&self) -> Option<ExternalCommandQueue> {
        match &self.queue {
            CommandQueueStorage::ExternalOwner(queue)
            | CommandQueueStorage::ExternalBorrowed(queue) => Some(queue.clone()),
            CommandQueueStorage::Owned(_) => None,
        }
    }

    pub(crate) fn finalize_external_owner(&mut self) -> Commands<'static> {
        let CommandQueueStorage::ExternalOwner(queue) = &self.queue else {
            panic!("external command owner finalization requires runtime command owner");
        };
        queue.active.set(false);
        let staged_queue = queue.drain();
        Commands {
            queue: CommandQueueStorage::Owned(staged_queue),
            _marker: PhantomData,
        }
    }
}

impl<'world> Commands<'world> {
    fn push_erased(&mut self, command: Box<dyn super::deferred::ErasedDeferredCommand>) {
        match &mut self.queue {
            CommandQueueStorage::Owned(queue) => queue.push(command),
            CommandQueueStorage::ExternalOwner(queue) => queue.queue.borrow_mut().push(command),
            CommandQueueStorage::ExternalBorrowed(queue) => {
                queue.assert_active();
                queue.queue.borrow_mut().push(command);
            }
        }
    }

    fn into_queue(self) -> CommandQueue {
        match self.queue {
            CommandQueueStorage::Owned(queue) => queue,
            CommandQueueStorage::ExternalOwner(queue) => queue.drain(),
            CommandQueueStorage::ExternalBorrowed(queue) => {
                queue.assert_active();
                queue.drain()
            }
        }
    }

    pub fn queue<F>(&mut self, command: F)
    where
        F: FnOnce(&mut World) -> Result<(), CommandError> + 'static,
    {
        self.push_erased(Box::new(command));
    }

    pub fn batch<F>(&mut self, build: F)
    where
        F: FnOnce(&mut BatchCommands),
    {
        let mut batch = BatchCommands::new();
        build(&mut batch);
        self.queue(move |world| batch.apply(world));
    }

    pub fn spawn<B: Bundle + 'static>(&mut self, bundle: B) {
        self.queue(move |world: &mut World| {
            let _ = world.spawn(bundle)?;
            Ok(())
        });
    }

    pub fn despawn(&mut self, entity: Entity) {
        self.queue(move |world: &mut World| {
            world.despawn(entity)?;
            Ok(())
        });
    }

    pub fn insert<B: Bundle + 'static>(&mut self, entity: Entity, bundle: B) {
        self.queue(move |world: &mut World| {
            world.insert(entity, bundle)?;
            Ok(())
        });
    }

    pub fn remove<B: Bundle + 'static>(&mut self, entity: Entity) {
        self.queue(move |world: &mut World| {
            let _: B = world.remove(entity)?;
            Ok(())
        });
    }

    pub fn apply(self, world: &mut World) -> Result<(), CommandError> {
        for command in self.into_queue() {
            command.apply_erased(world)?;
        }
        Ok(())
    }
}

impl Default for Commands<'static> {
    fn default() -> Self {
        Self::new()
    }
}
