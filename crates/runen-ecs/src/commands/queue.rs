use super::deferred::ErasedDeferredCommand;

pub(crate) type CommandQueue = Vec<Box<dyn ErasedDeferredCommand>>;
