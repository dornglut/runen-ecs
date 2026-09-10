//! File: crates/runen-ecs/src/reflect/type_info.rs
//! Purpose: Core reflected type metadata.

use crate::reflect::EnumInfo;
use crate::reflect::StructInfo;

#[derive(Debug, Clone, Copy)]
pub enum ReflectShape {
    Opaque,
    Struct(StructInfo),
    Enum(EnumInfo),
}

#[derive(Debug, Clone, Copy)]
pub struct TypeInfo {
    pub rust_name: &'static str,
    pub display_name: &'static str,
    pub shape: ReflectShape,
}

impl TypeInfo {
    pub const fn new(
        rust_name: &'static str,
        display_name: &'static str,
        shape: ReflectShape,
    ) -> Self {
        Self {
            rust_name,
            display_name,
            shape,
        }
    }

    pub fn struct_info(&self) -> Option<StructInfo> {
        match self.shape {
            ReflectShape::Opaque => None,
            ReflectShape::Struct(info) => Some(info),
            ReflectShape::Enum(_) => None,
        }
    }

    pub fn enum_info(&self) -> Option<EnumInfo> {
        match self.shape {
            ReflectShape::Opaque => None,
            ReflectShape::Struct(_) => None,
            ReflectShape::Enum(info) => Some(info),
        }
    }
}
