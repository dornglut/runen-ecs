use crate::query::QueryAccess;
use crate::scheduler::system::ParamSlotDescriptor;
use crate::world::WorldAuthority;
use crate::{Commands, ResourceError, World};
use std::marker::PhantomData;
use std::ptr::NonNull;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SystemParamError {
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error("invalid system param extraction for {param}: {reason}")]
    InvalidExtraction {
        param: &'static str,
        reason: &'static str,
    },
    #[error("runtime context error: {0}")]
    RuntimeContext(&'static str),
}

/// The normalized deferred-recorder capability of one [`SystemParam`] graph.
///
/// This is parameter metadata, not World access metadata. The runtime uses only
/// the non-`None` projection when deriving semantic publication frontiers.
#[doc(hidden)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DeferredRecorderClass {
    None,
    LocalDeferred,
}

impl DeferredRecorderClass {
    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::LocalDeferred, _) | (_, Self::LocalDeferred) => Self::LocalDeferred,
            _ => Self::None,
        }
    }

    pub const fn is_deferred_producing(self) -> bool {
        matches!(self, Self::LocalDeferred)
    }
}

/// Invocation-scoped extraction context owned by the RunenECS runtime.
///
/// Safe system code never constructs this value. It is public only because the
/// low-level [`SystemParam`] contract must remain reachable by downstream derive
/// expansion and the maintained engine-owned exclusive-world parameter.
#[doc(hidden)]
#[derive(Copy, Clone)]
pub struct SystemParamContext<'world> {
    authority: WorldAuthority<'world>,
    commands: NonNull<Commands<'static>>,
    _marker: PhantomData<&'world mut World>,
}

impl<'world> SystemParamContext<'world> {
    pub(crate) fn new(world: &'world mut World, commands: &'world mut Commands<'static>) -> Self {
        Self {
            authority: WorldAuthority::new(world),
            commands: NonNull::from(commands),
            _marker: PhantomData,
        }
    }

    pub(crate) fn query(self) -> crate::world::QueryCapability<'world> {
        self.authority.query()
    }

    pub(crate) fn resource<T: crate::Resource>(
        self,
    ) -> Result<crate::world::ResourceCapability<'world, T>, SystemParamError> {
        Ok(self.authority.resource::<T>()?)
    }

    pub(crate) fn resource_mut<T: crate::Resource>(
        self,
    ) -> Result<crate::world::ResourceCapability<'world, T>, SystemParamError> {
        Ok(self.authority.resource_mut::<T>()?)
    }

    /// # Safety
    /// The caller must have declared exclusive-world access and must not retain
    /// any sibling world capability.
    pub unsafe fn world_mut(self) -> &'world mut World {
        unsafe { self.authority.world_mut() }
    }

    pub(crate) fn commands(self) -> Commands<'world> {
        // Safety: the runtime constructs this pointer from the live command
        // owner and keeps it valid until extraction finishes. Only a shared
        // owner read is needed to clone its external queue; no mutable owner
        // reference is manufactured from the copied context.
        let queue = unsafe {
            self.commands
                .as_ref()
                .external_queue()
                .expect("command owner must provide an external queue")
        };
        Commands::from_external(queue)
    }
}

/// Framework-owned low-level system-parameter implementation contract.
///
/// Safe user composition uses the built-in parameters and
/// `#[derive(SystemParam)]`. Manual implementations are unsupported low-level
/// code because access metadata, raw extraction pointers, and cached state
/// participate directly in the runtime's aliasing and lifetime proof.
///
/// # Safety
///
/// Implementors must keep `State` lifetime-independent, report every immediate
/// borrow in `access`, and ensure each returned item is valid only for the
/// invocation/state lifetimes supplied to `extract`.
#[doc(hidden)]
pub unsafe trait SystemParam: Sized {
    type State: 'static;
    type Item<'world, 'state>;

    fn init_state(world: &mut World) -> Result<Self::State, SystemParamError>;
    fn deferred_recorder_class() -> DeferredRecorderClass {
        DeferredRecorderClass::None
    }
    fn access(state: &Self::State) -> QueryAccess;
    fn slot_descriptor() -> ParamSlotDescriptor {
        let type_name = std::any::type_name::<Self>();
        ParamSlotDescriptor::leaf("unknown", type_name, type_name)
    }

    /// # Safety
    ///
    /// `context` belongs to the current system invocation. Implementors may
    /// only access World domains described by `Self::access(state)`, may not
    /// extend references beyond the corresponding GAT lifetimes, and must
    /// preserve the runtime-validated aliasing contract.
    unsafe fn extract<'world, 'state>(
        state: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError>;
}

/// Framework-owned proof that a system parameter's cached state and access
/// shape are eligible for a future worker-safe invocation.
///
/// This is deliberately separate from [`SystemParam`]: the serial extraction
/// context remains invoker-thread-local, and this proof does not authorize
/// moving that context or executing a system on a worker today.
///
/// # Safety
///
/// An implementation must ensure that the parameter's cached [`SystemParam::State`]
/// can be moved to another thread and that every value reachable through its
/// declared access shape satisfies the exact `Send`/`Sync` requirements of the
/// maintained parameter form. It must not use this proof to widen the
/// parameter's access metadata or to transfer invocation-scoped references.
#[doc(hidden)]
pub unsafe trait TransferableSystemParam: SystemParam
where
    Self::State: Send,
{
}
