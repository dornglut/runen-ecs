//! File: crates/runen-ecs/src/reflect/primitives.rs
//! Purpose: Primitive opaque reflection implementations.

use crate::reflect::{Reflect, ReflectShape, TypeInfo};

macro_rules! impl_primitive_reflect {
    ($ty:ty, $stable_name:expr) => {
        impl Reflect for $ty {
            fn type_info() -> TypeInfo
            where
                Self: Sized,
            {
                TypeInfo::new(
                    std::any::type_name::<Self>(),
                    $stable_name,
                    ReflectShape::Opaque,
                )
            }
        }
    };
}

impl_primitive_reflect!(bool, "bool");
impl_primitive_reflect!(f32, "f32");
impl_primitive_reflect!(f64, "f64");
impl_primitive_reflect!(i8, "i8");
impl_primitive_reflect!(i16, "i16");
impl_primitive_reflect!(i32, "i32");
impl_primitive_reflect!(i64, "i64");
impl_primitive_reflect!(isize, "isize");
impl_primitive_reflect!(u8, "u8");
impl_primitive_reflect!(u16, "u16");
impl_primitive_reflect!(u32, "u32");
impl_primitive_reflect!(u64, "u64");
impl_primitive_reflect!(usize, "usize");
impl_primitive_reflect!(String, "String");
