use super::deferred::{ErasedDeferredCommand, ErasedTransferableDeferredCommand};

pub(crate) type CommandQueue = Vec<Box<dyn ErasedDeferredCommand>>;
pub(crate) type TransferableCommandQueue = Vec<Box<dyn ErasedTransferableDeferredCommand>>;
