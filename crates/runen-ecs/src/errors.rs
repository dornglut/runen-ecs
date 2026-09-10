use crate::Entity;
use crate::scheduler::plan::ScheduleValidationError;
use crate::system::SystemParamError;
use std::error::Error;
use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Entity(#[from] EntityError),
    #[error(transparent)]
    EntityAllocation(#[from] EntityAllocationError),
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

/// Structured envelope for failures owned by the ECS runtime boundary.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime setup failed: {message}")]
    Setup { message: String },
    #[error(transparent)]
    Schedule(#[from] ScheduleValidationError),
    #[error(transparent)]
    Param(#[from] SystemParamError),
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
