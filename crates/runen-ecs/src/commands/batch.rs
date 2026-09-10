use super::queue::CommandQueue;
use crate::bundle::Bundle;
use crate::entity::Entity;
use crate::errors::CommandError;
use crate::world::World;

/// Ordered group of deferred commands.
///
/// A batch is not a transaction: commands completed before the first error stay
/// committed, the failing command is responsible for its own atomicity, and
/// later commands are not executed.
pub struct BatchCommands {
    queue: CommandQueue,
}

impl BatchCommands {
    pub fn new() -> Self {
        Self { queue: Vec::new() }
    }

    pub fn queue<F>(&mut self, command: F)
    where
        F: FnOnce(&mut World) -> Result<(), CommandError> + 'static,
    {
        self.queue.push(Box::new(command));
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

    /// Applies commands in insertion order and stops on the first error.
    ///
    /// Successfully completed commands remain committed. The failing command
    /// follows its own operation-level atomicity contract, and commands after it
    /// are not executed. Consuming the batch prevents replay of attempted work.
    pub fn apply(self, world: &mut World) -> Result<(), CommandError> {
        for command in self.queue {
            command.apply_erased(world)?;
        }
        Ok(())
    }
}

impl Default for BatchCommands {
    fn default() -> Self {
        Self::new()
    }
}
