mod capability;
mod change_tracking;
mod component_indexes;
mod entity_handles;
mod mutation_journal;
mod parallel;
mod reflection;
mod runtime;
mod state;

pub mod component;
pub mod entity;
pub mod resource;

pub use change_tracking::ChangeCursor;
pub use entity_handles::{EntityMut, EntityRef, Mut};
pub use state::World;

pub(crate) use capability::{
    QueryCapability, ResourceCapability, ResourceMutationCapability, WorldAuthority,
};
pub(crate) use change_tracking::{FrameworkInvariantKind, framework_invariant_kind};
pub(crate) use mutation_journal::{ConcurrentMutationCapacity, MutationJournal};
pub(crate) use parallel::{
    ParallelWorldLease, PreparedWorkerWorld, WorkerWorldAuthority, WorkerWorldBuilder,
};
