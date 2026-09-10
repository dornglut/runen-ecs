// Owner: RunenECS World Component - Introspection APIs
use crate::entity::Entity;
use crate::reflect::{ReflectValueMut, ReflectValueRef, TypeInfo};
use crate::world::World;
use std::any::TypeId;

impl World {
    pub fn reflected_component_type_info(&self, type_id: TypeId) -> Option<TypeInfo> {
        if !self.reflected_component_types.contains_key(&type_id) {
            return None;
        }
        self.type_registry.get(type_id)
    }

    pub fn has_reflected_component_type(&self, type_id: TypeId) -> bool {
        self.reflected_component_types.contains_key(&type_id)
    }

    pub fn reflected_component_type_ids(&self) -> Vec<TypeId> {
        self.reflected_component_order.clone()
    }

    pub fn has_component_type(&self, type_id: TypeId) -> bool {
        self.component_type_registry.contains_key(&type_id)
    }

    pub fn component_type_ids(&self) -> Vec<TypeId> {
        self.component_type_registry.keys().copied().collect()
    }

    pub fn entity_component_type_ids(&self, entity: Entity) -> Vec<TypeId> {
        let Some(location) = self.entity_locations.get(entity) else {
            return Vec::new();
        };

        self.archetype_registry
            .component_types(location.archetype_id)
            .map(|types| types.to_vec())
            .unwrap_or_default()
    }

    pub fn entity_reflected_component_types(&self, entity: Entity) -> Vec<TypeInfo> {
        self.entity_component_type_ids(entity)
            .into_iter()
            .filter_map(|type_id| self.reflected_component_type_info(type_id))
            .collect()
    }

    pub fn reflected_component_value_ref(
        &self,
        entity: Entity,
        type_id: TypeId,
    ) -> Option<ReflectValueRef<'_>> {
        let registration = self.reflected_component_types.get(&type_id)?;
        (registration.value_ref)(self, entity)
    }

    pub fn reflected_component_value_mut(
        &mut self,
        entity: Entity,
        type_id: TypeId,
    ) -> Option<ReflectValueMut<'_>> {
        let registration = self.reflected_component_types.get(&type_id).copied()?;
        (registration.value_mut)(self, entity)
    }

    pub fn entity_has_component_type(&self, entity: Entity, type_id: TypeId) -> bool {
        self.entity_component_type_ids(entity).contains(&type_id)
    }

    pub fn entity_component_count(&self, entity: Entity) -> usize {
        self.entity_component_type_ids(entity).len()
    }
}
