//! File: crates/runen-ecs/src/reflect/mod.rs
//! Purpose: ECS reflection foundation.

mod component_registration;
mod enum_info;
mod field_info;
mod primitives;
mod registry;
mod resource_registration;
mod struct_info;
mod traits;
mod type_info;
mod value;

pub(crate) use component_registration::{
    ReflectedComponentRegistration, reflected_component_registration,
};
pub use enum_info::{
    EnumCurrentVariant, EnumInfo, EnumSetUnitVariant, EnumVariantAt, EnumVariantInfo,
};
pub use field_info::{FieldGetMut, FieldGetRef, FieldInfo, FieldTypeInfo};
pub use registry::TypeRegistry;
pub(crate) use resource_registration::{
    ReflectedResourceRegistration, reflected_resource_registration,
};
pub use struct_info::{StructFieldAt, StructInfo};
pub use traits::Reflect;
pub use type_info::{ReflectShape, TypeInfo};
pub use value::{
    EnumValueMut, EnumValueRef, ReflectValueMut, ReflectValueRef, StructValueMut, StructValueRef,
};
