use crate::errors::QueryError;
use crate::query::QueryAccess;
use crate::scheduler::system::ParamSlotDescriptor;
use crate::world::{
    MutationJournal, ParallelWorldLease, PreparedWorkerWorld, WorkerWorldAuthority,
    WorkerWorldBuilder, WorldAuthority,
};
use crate::{Commands, LocalCommands, Relation, ResourceError, World};
use std::ptr::NonNull;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SystemParamError {
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error(transparent)]
    Query(#[from] QueryError),
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
    TransferableDeferred,
}

#[doc(hidden)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeferredRecorderConflict {
    local: DeferredRecorderClass,
    transferable: DeferredRecorderClass,
}

impl DeferredRecorderClass {
    pub const fn merge(self, other: Self) -> Result<Self, DeferredRecorderConflict> {
        match (self, other) {
            (Self::None, other) | (other, Self::None) => Ok(other),
            (Self::LocalDeferred, Self::LocalDeferred) => Ok(Self::LocalDeferred),
            (Self::TransferableDeferred, Self::TransferableDeferred) => {
                Ok(Self::TransferableDeferred)
            }
            (Self::LocalDeferred, Self::TransferableDeferred)
            | (Self::TransferableDeferred, Self::LocalDeferred) => Err(DeferredRecorderConflict {
                local: Self::LocalDeferred,
                transferable: Self::TransferableDeferred,
            }),
        }
    }

    pub const fn is_deferred_producing(self) -> bool {
        !matches!(self, Self::None)
    }
}

impl DeferredRecorderConflict {
    pub const fn local(self) -> DeferredRecorderClass {
        self.local
    }

    pub const fn transferable(self) -> DeferredRecorderClass {
        self.transferable
    }
}

impl std::fmt::Display for DeferredRecorderConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "LocalDeferred and TransferableDeferred deferred recorder capabilities cannot be mixed",
        )
    }
}

/// Invoker-side type-directed preparation context for one future worker
/// invocation. It owns no user-facing World handle and is consumed before the
/// prepared package can move to a worker.
#[doc(hidden)]
pub struct WorkerPrepareContext<'world> {
    builder: WorkerWorldBuilder<'world>,
}

impl<'world> WorkerPrepareContext<'world> {
    pub(crate) fn new(lease: &ParallelWorldLease<'world>) -> Self {
        Self {
            builder: lease.builder(),
        }
    }

    pub(crate) fn builder(&mut self) -> &mut WorkerWorldBuilder<'world> {
        &mut self.builder
    }

    pub(crate) fn finish(self) -> PreparedWorkerWorld<'world> {
        self.builder.finish()
    }
}

#[derive(Copy, Clone)]
enum SystemParamContextBacking<'world> {
    Serial {
        authority: WorldAuthority<'world>,
        mutation_journal: NonNull<MutationJournal>,
        local_commands: Option<NonNull<LocalCommands<'static>>>,
        commands: Option<NonNull<Commands<'static>>>,
    },
    Worker {
        authority: WorkerWorldAuthority<'world>,
        mutation_journal: NonNull<MutationJournal>,
        commands: Option<NonNull<Commands<'static>>>,
    },
}

/// Invocation-scoped extraction context owned by the RunenECS runtime.
///
/// Safe system code never constructs this value. Serial extraction contains the
/// serial World authority. Worker extraction is a distinct backing created only
/// from a prepared worker package and cannot reconstruct `&mut World` or
/// `LocalCommands`.
#[doc(hidden)]
#[derive(Copy, Clone)]
pub struct SystemParamContext<'world> {
    backing: SystemParamContextBacking<'world>,
}

impl<'world> SystemParamContext<'world> {
    pub(crate) fn new(
        world: &'world mut World,
        mutation_journal: &'world mut MutationJournal,
        local_commands: Option<&'world mut LocalCommands<'static>>,
        commands: Option<&'world mut Commands<'static>>,
    ) -> Self {
        Self {
            backing: SystemParamContextBacking::Serial {
                authority: WorldAuthority::new(world),
                mutation_journal: NonNull::from(mutation_journal),
                local_commands: local_commands.map(NonNull::from),
                commands: commands.map(NonNull::from),
            },
        }
    }

    pub(crate) fn new_worker(
        authority: WorkerWorldAuthority<'world>,
        mutation_journal: &'world mut MutationJournal,
        commands: Option<&'world mut Commands<'static>>,
    ) -> Self {
        Self {
            backing: SystemParamContextBacking::Worker {
                authority,
                mutation_journal: NonNull::from(mutation_journal),
                commands: commands.map(NonNull::from),
            },
        }
    }

    pub(crate) fn query(self) -> crate::world::QueryCapability<'world> {
        match self.backing {
            SystemParamContextBacking::Serial {
                authority,
                mutation_journal,
                ..
            } => authority.query_with_journal(mutation_journal),
            SystemParamContextBacking::Worker {
                authority,
                mutation_journal,
                ..
            } => authority.query_with_journal(mutation_journal),
        }
    }

    pub(crate) fn query_for<Q: 'static, F: 'static>(self) -> crate::world::QueryCapability<'world> {
        match self.backing {
            SystemParamContextBacking::Serial {
                authority,
                mutation_journal,
                ..
            } => authority.query_with_journal(mutation_journal),
            SystemParamContextBacking::Worker {
                authority,
                mutation_journal,
                ..
            } => authority.query_with_journal_for::<Q, F>(mutation_journal),
        }
    }

    pub(crate) fn resource<T: crate::Resource>(
        self,
    ) -> Result<crate::world::ResourceCapability<'world, T>, SystemParamError> {
        match self.backing {
            SystemParamContextBacking::Serial { authority, .. } => Ok(authority.resource::<T>()?),
            SystemParamContextBacking::Worker { authority, .. } => Ok(authority.resource::<T>()?),
        }
    }

    pub(crate) fn resource_mut<T: crate::Resource>(
        self,
    ) -> Result<crate::world::ResourceCapability<'world, T>, SystemParamError> {
        match self.backing {
            SystemParamContextBacking::Serial {
                authority,
                mutation_journal,
                ..
            } => Ok(authority.resource_mut::<T>(mutation_journal)?),
            SystemParamContextBacking::Worker {
                authority,
                mutation_journal,
                ..
            } => Ok(authority.resource_mut::<T>(mutation_journal)?),
        }
    }

    pub(crate) fn relation<R: Relation>(self) -> crate::world::RelationReadCapability<'world, R> {
        match self.backing {
            SystemParamContextBacking::Serial { authority, .. } => authority.relation::<R>(),
            SystemParamContextBacking::Worker { authority, .. } => authority.relation::<R>(),
        }
    }

    pub(crate) fn relation_mut<R: Relation>(
        self,
    ) -> crate::world::RelationWriteCapability<'world, R> {
        match self.backing {
            SystemParamContextBacking::Serial { authority, .. } => authority.relation_mut::<R>(),
            SystemParamContextBacking::Worker { authority, .. } => authority.relation_mut::<R>(),
        }
    }

    /// # Safety
    /// The caller must have declared exclusive-world access and must not retain
    /// any sibling world capability. Worker contexts can never satisfy this
    /// contract and therefore reject the operation.
    pub unsafe fn world_mut(self) -> &'world mut World {
        match self.backing {
            SystemParamContextBacking::Serial { authority, .. } => unsafe { authority.world_mut() },
            SystemParamContextBacking::Worker { .. } => {
                panic!("prepared worker context cannot yield exclusive World access")
            }
        }
    }

    pub(crate) fn local_commands(self) -> LocalCommands<'world> {
        let SystemParamContextBacking::Serial { local_commands, .. } = self.backing else {
            panic!("prepared worker context cannot yield LocalCommands");
        };
        // Safety: the runtime constructs this pointer from the live command
        // owner and keeps it valid until extraction finishes. Only a shared
        // owner read is needed to clone its external queue.
        let queue = unsafe {
            local_commands
                .expect("local command owner must be available for LocalCommands")
                .as_ref()
                .external_queue()
                .expect("command owner must provide an external queue")
        };
        LocalCommands::from_external(queue)
    }

    pub(crate) fn commands(self) -> Commands<'world> {
        let commands = match self.backing {
            SystemParamContextBacking::Serial { commands, .. }
            | SystemParamContextBacking::Worker { commands, .. } => commands,
        };
        // Safety: the runtime constructs this pointer from a live command owner
        // on the current invocation thread and keeps it valid until extraction
        // finishes. Worker owners are constructed on the worker itself.
        let queue = unsafe {
            commands
                .expect("command owner must be available for Commands")
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

    fn init_state() -> Result<Self::State, SystemParamError>;
    fn deferred_recorder_class() -> Result<DeferredRecorderClass, DeferredRecorderConflict> {
        Ok(DeferredRecorderClass::None)
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

/// Framework-owned proof that a system parameter's cached state and concrete
/// payload/metadata access shape can be prepared for worker execution.
///
/// The preparation hook runs on the invoker under the structural-freeze lease
/// while the concrete parameter type is still known. It must place only narrow
/// worker-safe projections into `context`; scheduler `QueryAccess` metadata by
/// itself is never sufficient to implement this proof.
///
/// # Safety
///
/// Implementations must preserve the exact `Send`/`Sync` requirements of every
/// payload reachable through the parameter and prepare every metadata domain
/// later consulted by normal `SystemParam::extract` on the worker. They must not
/// move the serial `SystemParamContext`, World authority, LocalCommands, or a
/// live invoker-created Commands owner to the worker.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    note = "normal system registration requires every system parameter to satisfy RunenECS's transferable proof; use `.on_invoker_thread()` when a system intentionally uses thread-bound parameters",
    note = "`TransferableSystemParam` is framework-owned unsafe proof plumbing; application code should not implement it manually"
)]
pub unsafe trait TransferableSystemParam: SystemParam
where
    Self::State: Send,
{
    fn prepare_worker(
        _state: &Self::State,
        _context: &mut WorkerPrepareContext<'_>,
    ) -> Result<(), SystemParamError> {
        Err(SystemParamError::RuntimeContext(
            "transferable system param has no worker preparation proof",
        ))
    }
}
