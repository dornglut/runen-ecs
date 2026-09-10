// Owner: RunenECS World - Component Secondary Index Types
use super::World;
use crate::component::Component;
use crate::entity::Entity;
use std::any::{Any, TypeId};
use std::collections::BTreeMap;

pub(super) const DEFAULT_COMPONENT_INDEX_NAME: &str = "__default";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ComponentIndexKey {
    pub(super) component_type: TypeId,
    key_type: TypeId,
    name: String,
}

impl ComponentIndexKey {
    pub(super) fn new(component_type: TypeId, key_type: TypeId, name: impl Into<String>) -> Self {
        let mut name = name.into();
        name = name.trim().to_string();
        if name.is_empty() {
            name = DEFAULT_COMPONENT_INDEX_NAME.to_string();
        }
        Self {
            component_type,
            key_type,
            name,
        }
    }
}

pub(super) trait ComponentIndexStorage {
    fn mark_dirty(&mut self);
    fn rebuild(&mut self, world: &World);
    fn as_any(&self) -> &dyn Any;
}

pub(super) struct ComponentSecondaryIndex<T: Component, K: Ord + Clone + 'static> {
    entries: BTreeMap<K, Vec<Entity>>,
    extractor: fn(&T) -> K,
    dirty: bool,
}

impl<T: Component, K: Ord + Clone + 'static> ComponentSecondaryIndex<T, K> {
    pub(super) fn new(extractor: fn(&T) -> K) -> Self {
        Self {
            entries: BTreeMap::new(),
            extractor,
            dirty: true,
        }
    }

    pub(super) fn entities_for(&self, key: &K) -> Vec<Entity> {
        self.entries.get(key).cloned().unwrap_or_default()
    }

    pub(super) fn first_entity_for(&self, key: &K) -> Option<Entity> {
        self.entries
            .get(key)
            .and_then(|entities| entities.first())
            .copied()
    }
}

impl<T: Component, K: Ord + Clone + 'static> ComponentIndexStorage
    for ComponentSecondaryIndex<T, K>
{
    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn rebuild(&mut self, world: &World) {
        if !self.dirty {
            return;
        }

        self.entries.clear();
        let mut entities = Vec::new();
        world.matching_entities_into(&[TypeId::of::<T>()], &[], &mut entities);
        for entity in entities {
            let Some(component) = world.get::<T>(entity) else {
                continue;
            };
            let key = (self.extractor)(component);
            self.entries.entry(key).or_default().push(entity);
        }
        self.dirty = false;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
