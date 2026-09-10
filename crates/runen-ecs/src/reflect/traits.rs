//! File: crates/runen-ecs/src/reflect/traits.rs
//! Purpose: Reflection trait contracts for ECS types.

use crate::reflect::{ReflectValueMut, ReflectValueRef, TypeInfo};

pub trait Reflect: 'static {
    fn type_info() -> TypeInfo
    where
        Self: Sized;

    fn display_name() -> &'static str
    where
        Self: Sized,
    {
        Self::type_info().display_name
    }

    fn reflect_ref(&self) -> ReflectValueRef<'_>
    where
        Self: Sized,
    {
        ReflectValueRef::new(self)
    }

    fn reflect_mut(&mut self) -> ReflectValueMut<'_>
    where
        Self: Sized,
    {
        ReflectValueMut::new(self)
    }
}
