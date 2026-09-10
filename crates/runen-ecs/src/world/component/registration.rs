// Owner: RunenECS World Component - Registration and Secondary Index APIs
use crate::bundle::BundleComponentDescriptor;
use crate::component::Component;
use crate::entity::Entity;
use crate::world::World;
use crate::world::component_indexes::{
    ComponentIndexKey, ComponentSecondaryIndex, DEFAULT_COMPONENT_INDEX_NAME,
};
use std::any::TypeId;

impl World {
    pub(crate) fn __register_component<T: Component>(&mut self) {
        self.__register_component_descriptor(BundleComponentDescriptor::of::<T>());
    }

    pub(crate) fn __register_component_descriptor(
        &mut self,
        descriptor: BundleComponentDescriptor,
    ) {
        descriptor.register_storage(&mut self.archetype_registry);
        self.component_type_registry
            .entry(descriptor.component_type_id())
            .or_insert_with(|| descriptor.component_name());
    }

    pub(crate) fn register_bundle_descriptors(
        &mut self,
        descriptors: &[BundleComponentDescriptor],
    ) {
        for descriptor in descriptors {
            self.__register_component_descriptor(*descriptor);
        }
    }

    pub fn ensure_component_index<T: Component, K: Ord + Clone + 'static>(
        &mut self,
        extractor: fn(&T) -> K,
    ) -> bool {
        self.ensure_component_index_named::<T, K>(DEFAULT_COMPONENT_INDEX_NAME, extractor)
    }

    pub fn ensure_component_index_named<T: Component, K: Ord + Clone + 'static>(
        &mut self,
        name: impl Into<String>,
        extractor: fn(&T) -> K,
    ) -> bool {
        self.__register_component::<T>();
        let key = ComponentIndexKey::new(TypeId::of::<T>(), TypeId::of::<K>(), name);
        let mut indexes = self.component_indexes.borrow_mut();
        if indexes.contains_key(&key) {
            return false;
        }
        indexes.insert(
            key,
            Box::new(ComponentSecondaryIndex::<T, K>::new(extractor)),
        );
        drop(indexes);
        self.mark_component_indexes_dirty(TypeId::of::<T>());
        true
    }

    pub fn find_entity_by_index<T: Component, K: Ord + Clone + 'static>(
        &self,
        key: &K,
    ) -> Option<Entity> {
        self.find_entity_by_index_named::<T, K>(DEFAULT_COMPONENT_INDEX_NAME, key)
    }

    pub fn find_entity_by_index_named<T: Component, K: Ord + Clone + 'static>(
        &self,
        name: impl Into<String>,
        key: &K,
    ) -> Option<Entity> {
        let index_key = ComponentIndexKey::new(TypeId::of::<T>(), TypeId::of::<K>(), name);
        let mut indexes = self.component_indexes.borrow_mut();
        let index = indexes.get_mut(&index_key)?;
        index.rebuild(self);
        index
            .as_any()
            .downcast_ref::<ComponentSecondaryIndex<T, K>>()
            .and_then(|index| index.first_entity_for(key))
    }

    pub fn find_entities_by_index<T: Component, K: Ord + Clone + 'static>(
        &self,
        key: &K,
    ) -> Vec<Entity> {
        self.find_entities_by_index_named::<T, K>(DEFAULT_COMPONENT_INDEX_NAME, key)
    }

    pub fn find_entities_by_index_named<T: Component, K: Ord + Clone + 'static>(
        &self,
        name: impl Into<String>,
        key: &K,
    ) -> Vec<Entity> {
        let index_key = ComponentIndexKey::new(TypeId::of::<T>(), TypeId::of::<K>(), name);
        let mut indexes = self.component_indexes.borrow_mut();
        let Some(index) = indexes.get_mut(&index_key) else {
            return Vec::new();
        };
        index.rebuild(self);
        index
            .as_any()
            .downcast_ref::<ComponentSecondaryIndex<T, K>>()
            .map(|index| index.entities_for(key))
            .unwrap_or_default()
    }

    pub fn find_component_by_index<T: Component, K: Ord + Clone + 'static>(
        &self,
        key: &K,
    ) -> Option<&T> {
        self.find_component_by_index_named::<T, K>(DEFAULT_COMPONENT_INDEX_NAME, key)
    }

    pub fn find_component_by_index_named<T: Component, K: Ord + Clone + 'static>(
        &self,
        name: impl Into<String>,
        key: &K,
    ) -> Option<&T> {
        let entity = self.find_entity_by_index_named::<T, K>(name, key)?;
        self.get::<T>(entity)
    }
}
