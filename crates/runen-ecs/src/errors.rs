use crate::Entity;
use crate::scheduler::plan::ScheduleValidationError;
use crate::system::SystemParamError;
use std::error::Error;
use thiserror::Error;

#[derive(Debug, Error, Copy, Clone, PartialEq, Eq)]
pub enum ChangeCursorError {
    #[error("change cursor belongs to a different world")]
    ForeignWorld,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EntityAllocationError {
    #[error("entity index space exhausted")]
    IndexExhausted,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EntityError {
    #[error("entity {entity:?} belongs to a different world")]
    ForeignWorld { entity: Entity },
    #[error("entity {entity:?} is unknown")]
    UnknownEntity { entity: Entity },
    #[error("entity {entity:?} has a stale generation; current generation is {current_generation}")]
    StaleGeneration {
        entity: Entity,
        current_generation: u32,
    },
    #[error("entity {entity:?} was already freed")]
    AlreadyFreed { entity: Entity },
    #[error("entity {entity:?} is missing component {component}")]
    MissingComponent {
        entity: Entity,
        component: &'static str,
    },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ResourceError {
    #[error("resource {resource} does not exist")]
    Missing { resource: &'static str },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RelationError {
    #[error(transparent)]
    Entity(#[from] EntityError),
    #[error("relation {relation} forbids self-reference for entity {entity:?}")]
    SelfReference {
        relation: &'static str,
        entity: Entity,
    },
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Entity(#[from] EntityError),
    #[error(transparent)]
    EntityAllocation(#[from] EntityAllocationError),
    #[error(transparent)]
    Relation(#[from] RelationError),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QueryError {
    #[error("query expected exactly one result but found none")]
    NoResults,
    #[error("query expected exactly one result but found {count}")]
    MultipleResults { count: usize },
    #[error("query has conflicting {domain} borrows for {target}")]
    ConflictingBorrow {
        domain: &'static str,
        target: &'static str,
    },
}

/// A contiguous query segment could not be projected without changing query
/// semantics. Contiguous queries fail explicitly; they never fall back to
/// scalar iteration.
#[derive(Debug, Error, Copy, Clone, PartialEq, Eq)]
pub enum ContiguousQueryError {
    #[error("query data shape does not support contiguous segments")]
    UnsupportedQueryShape,
    #[error("query filter shape is not archetype-uniform")]
    UnsupportedFilterShape,
    #[error("contiguous query component types must be distinct")]
    AliasedComponentType,
    #[error("contiguous segments are unavailable from worker query capabilities")]
    WorkerCapability,
    #[error("mutable contiguous projections require an exclusive World borrow")]
    MutableWorldRequired,
    #[error("component storage is not aligned with its archetype entity rows")]
    StorageInvariant,
    #[error("component type is not projected by this query")]
    ComponentNotProjected,
    #[error("component is not mutably projected by this query")]
    ComponentNotMutable,
}

/// Structured envelope for failures owned by the ECS runtime boundary.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime setup failed: {message}")]
    Setup { message: String },
    #[error(transparent)]
    Schedule(#[from] ScheduleValidationError),
    #[error("system '{system}' parameter failed: {source}")]
    Param {
        system: String,
        #[source]
        source: SystemParamError,
    },
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("system '{system}' failed: {source}")]
    System {
        system: String,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("runtime boundary callback failed: {source}")]
    Boundary {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("runtime invariant violated: {message}")]
    Invariant { message: &'static str },
}
