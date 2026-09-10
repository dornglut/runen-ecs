// Owner: RunenECS Storage - Archetype Registry and Location Tracking
mod location;
mod registry;

pub(crate) use location::{EntityLocation, EntityLocationMap};
pub(crate) use registry::{ArchetypeExecutionBinding, ArchetypeId, ArchetypeRegistry};
