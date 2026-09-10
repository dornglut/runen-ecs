//! File: crates/runen-ecs/src/reflect/registry.rs
//! Purpose: Explicit metadata registry for reflected ECS types.

use std::any::TypeId;
use std::collections::HashMap;

use crate::reflect::{Reflect, TypeInfo};

#[derive(Debug, Default)]
pub struct TypeRegistry {
    by_type_id: HashMap<TypeId, TypeInfo>,
    registration_order: Vec<TypeId>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            by_type_id: HashMap::new(),
            registration_order: Vec::new(),
        }
    }

    pub fn register<T>(&mut self) -> TypeInfo
    where
        T: Reflect,
    {
        let rust_type_id = TypeId::of::<T>();
        if let Some(existing) = self.by_type_id.get(&rust_type_id) {
            return *existing;
        }

        let type_info = T::type_info();
        self.by_type_id.insert(rust_type_id, type_info);
        self.registration_order.push(rust_type_id);
        type_info
    }

    pub fn get(&self, rust_type_id: TypeId) -> Option<TypeInfo> {
        self.by_type_id.get(&rust_type_id).copied()
    }

    pub fn contains(&self, rust_type_id: TypeId) -> bool {
        self.by_type_id.contains_key(&rust_type_id)
    }

    pub fn type_ids(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.registration_order.iter().copied()
    }

    pub fn types(&self) -> impl Iterator<Item = TypeInfo> + '_ {
        self.registration_order
            .iter()
            .filter_map(|type_id| self.by_type_id.get(type_id).copied())
    }
}
