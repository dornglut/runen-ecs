// Owner: RunenECS World Resource - Introspection APIs
use crate::reflect::{ReflectValueMut, ReflectValueRef, TypeInfo};
use crate::world::World;
use std::any::TypeId;

impl World {
    pub fn reflected_resource_type_info(&self, type_id: TypeId) -> Option<TypeInfo> {
        if !self.reflected_resource_types.contains_key(&type_id) {
            return None;
        }
        self.type_registry.get(type_id)
    }

    pub fn has_reflected_resource_type(&self, type_id: TypeId) -> bool {
        self.reflected_resource_types.contains_key(&type_id)
    }

    pub fn reflected_resource_type_ids(&self) -> Vec<TypeId> {
        self.reflected_resource_order.clone()
    }

    pub fn live_resource_type_ids(&self) -> Vec<TypeId> {
        self.resources.keys().copied().collect()
    }

    pub fn live_reflected_resource_types(&self) -> Vec<TypeInfo> {
        self.live_resource_type_ids()
            .into_iter()
            .filter_map(|type_id| self.reflected_resource_type_info(type_id))
            .collect()
    }

    pub fn reflected_resource_value_ref(&self, type_id: TypeId) -> Option<ReflectValueRef<'_>> {
        let registration = self.reflected_resource_types.get(&type_id)?;
        (registration.value_ref)(self)
    }

    pub fn reflected_resource_value_mut(&mut self, type_id: TypeId) -> Option<ReflectValueMut<'_>> {
        let registration = self.reflected_resource_types.get(&type_id).copied()?;
        (registration.value_mut)(self)
    }
}
