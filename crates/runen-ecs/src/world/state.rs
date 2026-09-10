// Owner: RunenECS World - World State and Construction
use super::change_tracking::RemovedComponentRecord;
use super::component_indexes::{ComponentIndexKey, ComponentIndexStorage};
use crate::entity::{Entity, EntityAllocator, WorldScopeId};
use crate::storage::{ArchetypeRegistry, EntityLocationMap};
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

pub struct World {
    pub(super) allocator: EntityAllocator,
    pub(super) alive_entities: BTreeSet<Entity>,

    pub(super) component_type_registry: HashMap<TypeId, &'static str>,
    pub(super) reflected_component_types:
        HashMap<TypeId, crate::reflect::ReflectedComponentRegistration>,
    pub(super) reflected_component_order: Vec<TypeId>,
    pub(super) reflected_resource_types:
        HashMap<TypeId, crate::reflect::ReflectedResourceRegistration>,
    pub(super) reflected_resource_order: Vec<TypeId>,
    pub(super) type_registry: crate::reflect::TypeRegistry,

    pub(super) resources: HashMap<TypeId, Box<dyn Any>>,

    pub(super) component_indexes:
        RefCell<HashMap<ComponentIndexKey, Box<dyn ComponentIndexStorage>>>,

    pub(super) archetype_registry: ArchetypeRegistry,
    pub(super) entity_locations: EntityLocationMap,

    pub(super) change_tick: super::change_tracking::ChangeCursor,
    pub(super) component_change_ticks: HashMap<TypeId, super::change_tracking::ChangeCursor>,
    pub(super) resource_change_ticks: HashMap<TypeId, super::change_tracking::ChangeCursor>,
    pub(super) removed_component_records: HashMap<TypeId, Vec<RemovedComponentRecord>>,
}

impl World {
    pub fn new() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            alive_entities: BTreeSet::new(),

            component_type_registry: HashMap::new(),
            reflected_component_types: HashMap::new(),
            reflected_component_order: Vec::new(),
            reflected_resource_types: HashMap::new(),
            reflected_resource_order: Vec::new(),
            type_registry: crate::reflect::TypeRegistry::new(),

            resources: HashMap::new(),

            component_indexes: RefCell::new(HashMap::new()),

            archetype_registry: ArchetypeRegistry::new(),
            entity_locations: Default::default(),

            change_tick: super::change_tracking::ChangeCursor::default(),
            component_change_ticks: HashMap::new(),
            resource_change_ticks: HashMap::new(),
            removed_component_records: HashMap::new(),
        }
    }

    pub(crate) const fn scope_id(&self) -> WorldScopeId {
        self.allocator.scope_id()
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
