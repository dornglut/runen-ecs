// Owner: RunenECS World Component - Reflection Registration APIs
use crate::reflect::Reflect;
use crate::world::World;
use std::any::TypeId;

impl World {
    pub fn register_reflected_component<T>(&mut self)
    where
        T: crate::Component + Reflect,
    {
        self.ensure_reflected_component_registered::<T>();
    }

    pub fn reflected_component_types(&self) -> Vec<crate::reflect::TypeInfo> {
        self.reflected_component_order
            .iter()
            .filter(|type_id| self.reflected_component_types.contains_key(type_id))
            .filter_map(|type_id| self.type_registry.get(*type_id))
            .collect()
    }

    pub(crate) fn ensure_reflected_component_registered<T>(&mut self)
    where
        T: crate::Component + Reflect,
    {
        self.__register_component::<T>();
        let type_id = TypeId::of::<T>();
        if self.reflected_component_types.contains_key(&type_id) {
            return;
        }

        let _ = self.type_registry.register::<T>();
        self.reflected_component_types.insert(
            type_id,
            crate::reflect::reflected_component_registration::<T>(),
        );
        self.reflected_component_order.push(type_id);
    }
}
