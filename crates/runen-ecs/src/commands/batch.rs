use super::queue::TransferableCommandQueue;
use crate::bundle::Bundle;
use crate::entity::Entity;
use crate::errors::CommandError;
use crate::world::{Relation, World};

/// Ordered group of transfer-safe deferred commands.
pub struct BatchCommands {
    queue: TransferableCommandQueue,
}

impl BatchCommands {
    pub fn new() -> Self {
        Self { queue: Vec::new() }
    }

    pub fn queue<F>(&mut self, command: F)
    where
        F: FnOnce(&mut World) -> Result<(), CommandError> + Send + 'static,
    {
        self.queue.push(Box::new(command));
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

    pub fn insert_relation<R: Relation>(&mut self, source: Entity, target: Entity) {
        self.queue(move |world: &mut World| {
            let _ = world.relations_mut::<R>().insert(source, target)?;
            Ok(())
        });
    }

    pub fn remove_relation<R: Relation>(&mut self, source: Entity, target: Entity) {
        self.queue(move |world: &mut World| {
            let _ = world.relations_mut::<R>().remove(source, target)?;
            Ok(())
        });
    }

    pub fn clear_relations<R: Relation>(&mut self, entity: Entity) {
        self.queue(move |world: &mut World| {
            let _ = world.relations_mut::<R>().clear_entity(entity)?;
            Ok(())
        });
    }

    pub(crate) fn apply(self, world: &mut World) -> Result<(), CommandError> {
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
