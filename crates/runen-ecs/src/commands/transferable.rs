use super::queue::TransferableCommandQueue;
use super::transferable_batch::TransferableBatchCommands;
use super::transferable_buffer::TransferableCommandBuffer;
use crate::bundle::Bundle;
use crate::entity::Entity;
use crate::errors::CommandError;
use crate::world::World;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;

/// Invocation-local recorder for transfer-safe deferred structural effects.
///
/// The live recorder is intentionally local to the invocation thread. Its
/// finalized buffer is the representation that preserves `Send` after erasure.
pub struct TransferableCommands<'world> {
    queue: TransferableQueueStorage,
    _marker: PhantomData<&'world mut World>,
}

enum TransferableQueueStorage {
    Owned(TransferableCommandQueue),
    ExternalOwner(ExternalTransferableCommandQueue),
    ExternalBorrowed(ExternalTransferableCommandQueue),
}

#[derive(Clone)]
pub(crate) struct ExternalTransferableCommandQueue {
    queue: Rc<RefCell<TransferableCommandQueue>>,
    active: Rc<Cell<bool>>,
}

impl ExternalTransferableCommandQueue {
    fn new() -> Self {
        Self {
            queue: Rc::new(RefCell::new(Vec::new())),
            active: Rc::new(Cell::new(true)),
        }
    }

    fn assert_active(&self) {
        assert!(
            self.active.get(),
            "transferable commands param escaped its system execution scope"
        );
    }

    fn drain(&self) -> TransferableCommandQueue {
        std::mem::take(&mut *self.queue.borrow_mut())
    }
}

impl TransferableCommands<'static> {
    pub fn new() -> Self {
        Self {
            queue: TransferableQueueStorage::Owned(Vec::new()),
            _marker: PhantomData,
        }
    }

    pub(crate) fn new_external_owner() -> Self {
        Self {
            queue: TransferableQueueStorage::ExternalOwner(ExternalTransferableCommandQueue::new()),
            _marker: PhantomData,
        }
    }

    pub(crate) fn from_external<'world>(
        queue: ExternalTransferableCommandQueue,
    ) -> TransferableCommands<'world> {
        TransferableCommands {
            queue: TransferableQueueStorage::ExternalBorrowed(queue),
            _marker: PhantomData,
        }
    }

    pub(crate) fn external_queue(&self) -> Option<ExternalTransferableCommandQueue> {
        match &self.queue {
            TransferableQueueStorage::ExternalOwner(queue)
            | TransferableQueueStorage::ExternalBorrowed(queue) => Some(queue.clone()),
            TransferableQueueStorage::Owned(_) => None,
        }
    }

    pub(crate) fn finalize_external_owner(&mut self) -> TransferableCommandBuffer {
        let TransferableQueueStorage::ExternalOwner(queue) = &self.queue else {
            panic!(
                "external transferable command owner finalization requires runtime command owner"
            );
        };
        queue.active.set(false);
        TransferableCommandBuffer::new(queue.drain())
    }
}

impl<'world> TransferableCommands<'world> {
    fn push_erased(
        &mut self,
        command: Box<dyn super::deferred::ErasedTransferableDeferredCommand>,
    ) {
        match &mut self.queue {
            TransferableQueueStorage::Owned(queue) => queue.push(command),
            TransferableQueueStorage::ExternalOwner(queue) => {
                queue.queue.borrow_mut().push(command)
            }
            TransferableQueueStorage::ExternalBorrowed(queue) => {
                queue.assert_active();
                queue.queue.borrow_mut().push(command);
            }
        }
    }

    fn into_queue(self) -> TransferableCommandQueue {
        match self.queue {
            TransferableQueueStorage::Owned(queue) => queue,
            TransferableQueueStorage::ExternalOwner(queue) => queue.drain(),
            TransferableQueueStorage::ExternalBorrowed(queue) => {
                queue.assert_active();
                queue.drain()
            }
        }
    }

    pub fn queue<F>(&mut self, command: F)
    where
        F: FnOnce(&mut World) -> Result<(), CommandError> + Send + 'static,
    {
        self.push_erased(Box::new(command));
    }

    pub fn batch<F>(&mut self, build: F)
    where
        F: FnOnce(&mut TransferableBatchCommands),
    {
        let mut batch = TransferableBatchCommands::new();
        build(&mut batch);
        self.push_erased(Box::new(move |world: &mut World| batch.apply(world)));
    }

    pub fn spawn<B: Bundle + Send + 'static>(&mut self, bundle: B) {
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

    pub fn insert<B: Bundle + Send + 'static>(&mut self, entity: Entity, bundle: B) {
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

impl Default for TransferableCommands<'static> {
    fn default() -> Self {
        Self::new()
    }
}
