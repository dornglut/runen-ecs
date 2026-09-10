//! File: crates/runen-ecs/src/reflect/field_info.rs
//! Purpose: Field metadata and field accessor function pointers.

use crate::reflect::{ReflectValueMut, ReflectValueRef, TypeInfo};

pub type FieldGetRef = for<'a> fn(&'a dyn std::any::Any) -> Option<ReflectValueRef<'a>>;
pub type FieldGetMut = for<'a> fn(&'a mut dyn std::any::Any) -> Option<ReflectValueMut<'a>>;
pub type FieldTypeInfo = fn() -> TypeInfo;

#[derive(Debug, Clone, Copy)]
pub struct FieldInfo {
    pub name: &'static str,
    pub display_name: &'static str,
    type_info: FieldTypeInfo,
    pub get_ref: FieldGetRef,
    pub get_mut: FieldGetMut,
}

impl FieldInfo {
    pub const fn new(
        name: &'static str,
        display_name: &'static str,
        type_info: FieldTypeInfo,
        get_ref: FieldGetRef,
        get_mut: FieldGetMut,
    ) -> Self {
        Self {
            name,
            display_name,
            type_info,
            get_ref,
            get_mut,
        }
    }

    pub fn type_info(&self) -> TypeInfo {
        (self.type_info)()
    }
}
