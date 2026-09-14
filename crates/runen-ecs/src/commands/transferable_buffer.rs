use super::queue::TransferableCommandQueue;
use crate::errors::CommandError;
use crate::world::World;

/// Finalized transfer-safe deferred effects.
pub struct TransferableCommandBuffer {
    queue: TransferableCommandQueue,
}

impl TransferableCommandBuffer {
    pub(crate) fn new(queue: TransferableCommandQueue) -> Self {
        Self { queue }
    }

    pub(crate) fn apply(self, world: &mut World) -> Result<(), CommandError> {
        for command in self.queue {
            command.apply_erased(world)?;
        }
        Ok(())
    }
}

fn assert_send<T: Send>() {}

const _: fn() = assert_send::<TransferableCommandBuffer>;
