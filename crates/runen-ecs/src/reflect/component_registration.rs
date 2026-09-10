//! File: crates/runen-ecs/src/reflect/component_registration.rs
//! Purpose: ECS-owned reflected component role/access capability metadata.

use crate::component::Component;
use crate::entity::Entity;
use crate::reflect::{Reflect, ReflectValueMut, ReflectValueRef};
use crate::world::World;

pub(crate) type ComponentValueRefAccessor =
    for<'a> fn(&'a World, Entity) -> Option<ReflectValueRef<'a>>;
pub(crate) type ComponentValueMutAccessor =
    for<'a> fn(&'a mut World, Entity) -> Option<ReflectValueMut<'a>>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReflectedComponentRegistration {
    pub(crate) value_ref: ComponentValueRefAccessor,
    pub(crate) value_mut: ComponentValueMutAccessor,
}

impl ReflectedComponentRegistration {
    pub(crate) const fn new(
        value_ref: ComponentValueRefAccessor,
        value_mut: ComponentValueMutAccessor,
    ) -> Self {
        Self {
            value_ref,
            value_mut,
        }
    }
}

pub(crate) fn reflected_component_registration<T>() -> ReflectedComponentRegistration
where
    T: Reflect + Component,
{
    fn value_ref_impl<T>(world: &World, entity: Entity) -> Option<ReflectValueRef<'_>>
    where
        T: Reflect + Component,
    {
        world.get::<T>(entity).map(Reflect::reflect_ref)
    }

    fn value_mut_impl<T>(world: &mut World, entity: Entity) -> Option<ReflectValueMut<'_>>
    where
        T: Reflect + Component,
    {
        let value = world.get_mut::<T>(entity)?;
        Some(Reflect::reflect_mut(value.into_inner()))
    }

    ReflectedComponentRegistration::new(value_ref_impl::<T>, value_mut_impl::<T>)
}
