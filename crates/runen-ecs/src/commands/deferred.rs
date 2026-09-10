use crate::errors::CommandError;
use crate::world::World;
pub(crate) trait ErasedDeferredCommand {
    fn apply_erased(self: Box<Self>, world: &mut World) -> Result<(), CommandError>;
}

impl<F> ErasedDeferredCommand for F
where
    F: FnOnce(&mut World) -> Result<(), CommandError> + 'static,
{
    fn apply_erased(self: Box<Self>, world: &mut World) -> Result<(), CommandError> {
        (*self)(world)
    }
}
