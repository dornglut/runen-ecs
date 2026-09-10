// Owner: RunenECS World Resource - Reflection Registration APIs
use crate::component::Resource;
use crate::reflect::Reflect;
use crate::world::World;
use std::any::TypeId;

impl World {
    pub fn register_reflected_resource<T>(&mut self)
    where
        T: Resource + Reflect,
    {
        self.ensure_reflected_resource_registered::<T>();
    }

    pub fn reflected_resource_types(&self) -> Vec<crate::reflect::TypeInfo> {
        self.reflected_resource_order
            .iter()
            .filter(|type_id| self.reflected_resource_types.contains_key(type_id))
            .filter_map(|type_id| self.type_registry.get(*type_id))
            .collect()
    }

    pub(crate) fn ensure_reflected_resource_registered<T>(&mut self)
    where
        T: crate::Resource + crate::reflect::Reflect,
    {
        let type_id = TypeId::of::<T>();
        if self.reflected_resource_types.contains_key(&type_id) {
            return;
        }

        let _ = self.type_registry.register::<T>();
        self.reflected_resource_types.insert(
            type_id,
            crate::reflect::reflected_resource_registration::<T>(),
        );
        self.reflected_resource_order.push(type_id);
    }
}
