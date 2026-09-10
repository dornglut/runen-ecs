mod capability;
mod change_tracking;
mod component_indexes;
mod entity_handles;
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
