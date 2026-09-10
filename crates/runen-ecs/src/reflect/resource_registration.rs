//! File: crates/runen-ecs/src/reflect/resource_registration.rs
//! Purpose: ECS-owned reflected resource role/access capability metadata.

use crate::component::Resource;
use crate::reflect::{Reflect, ReflectValueMut, ReflectValueRef};
use crate::world::World;

pub(crate) type ResourceValueRefAccessor = for<'a> fn(&'a World) -> Option<ReflectValueRef<'a>>;
pub(crate) type ResourceValueMutAccessor = for<'a> fn(&'a mut World) -> Option<ReflectValueMut<'a>>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReflectedResourceRegistration {
    pub(crate) value_ref: ResourceValueRefAccessor,
    pub(crate) value_mut: ResourceValueMutAccessor,
}

impl ReflectedResourceRegistration {
    pub(crate) const fn new(
        value_ref: ResourceValueRefAccessor,
        value_mut: ResourceValueMutAccessor,
    ) -> Self {
        Self {
            value_ref,
            value_mut,
        }
    }
}

pub(crate) fn reflected_resource_registration<T>() -> ReflectedResourceRegistration
where
    T: Reflect + Resource,
{
    fn value_ref_impl<T>(world: &World) -> Option<ReflectValueRef<'_>>
    where
        T: Reflect + Resource,
    {
        world.resource::<T>().ok().map(Reflect::reflect_ref)
    }

    fn value_mut_impl<T>(world: &mut World) -> Option<ReflectValueMut<'_>>
    where
        T: Reflect + Resource,
    {
        world
            .resource_mut::<T>()
            .ok()
            .map(|value| Reflect::reflect_mut(value))
    }

    ReflectedResourceRegistration::new(value_ref_impl::<T>, value_mut_impl::<T>)
}
