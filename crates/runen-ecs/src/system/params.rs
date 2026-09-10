use super::extract::{SystemParam, SystemParamContext, SystemParamError};
use crate::Commands;
use crate::World;
use crate::component::{Component, Resource};
use crate::query::{
    Query, QueryAccess, QueryFilter, QuerySpec, QueryState, RemovedQuery, RemovedState,
};
use crate::scheduler::system::ParamSlotDescriptor;
use crate::world::{ResourceCapability, ResourceMutationCapability};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

/// Exclusive access to the complete world for systems that must coordinate
/// multiple ECS domains atomically. The runtime rejects all sibling immediate
/// borrows before this parameter is extracted.
pub struct WorldMut<'world> {
    world: NonNull<World>,
    _marker: PhantomData<&'world mut World>,
}

impl<'world> Deref for WorldMut<'world> {
    type Target = World;

    fn deref(&self) -> &World {
        // Safety: the runtime creates this value only after validating
        // exclusive-world access for the invocation.
        unsafe { self.world.as_ref() }
    }
}

impl<'world> DerefMut for WorldMut<'world> {
    fn deref_mut(&mut self) -> &mut World {
        // Safety: the runtime creates this value only after validating
        // exclusive-world access for the invocation.
        unsafe { self.world.as_mut() }
    }
}

unsafe impl<'param> SystemParam for WorldMut<'param> {
    type State = ();
    type Item<'world, 'state> = WorldMut<'world>;

    fn init_state(_: &mut World) -> Result<Self::State, SystemParamError> {
        Ok(())
    }

    fn access(_: &Self::State) -> QueryAccess {
        QueryAccess::exclusive_world()
    }

    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf("world_mut", "WorldMut", std::any::type_name::<Self>())
    }

    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        let world = unsafe { context.world_mut() };
        Ok(WorldMut {
            world: NonNull::from(world),
            _marker: PhantomData,
        })
    }
}

pub struct Res<'world, T: Resource> {
    value: NonNull<T>,
    _marker: PhantomData<&'world T>,
}
impl<'world, T: Resource> Res<'world, T> {
    pub(crate) fn new(capability: ResourceCapability<'world, T>) -> Self {
        Self {
            value: capability.value(),
            _marker: PhantomData,
        }
    }
}
impl<'world, T: Resource> Deref for Res<'world, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { self.value.as_ref() }
    }
}

pub struct ResMut<'world, T: Resource> {
    value: NonNull<T>,
    mutation: ResourceMutationCapability<'world>,
    _marker: PhantomData<&'world mut T>,
}
impl<'world, T: Resource> ResMut<'world, T> {
    pub(crate) fn new(capability: ResourceCapability<'world, T>) -> Self {
        Self {
            value: capability.value(),
            mutation: capability
                .mutation()
                .expect("mutable resource capability must track mutation"),
            _marker: PhantomData,
        }
    }
}
impl<'world, T: Resource> Deref for ResMut<'world, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { self.value.as_ref() }
    }
}
impl<'world, T: Resource> DerefMut for ResMut<'world, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.mutation.mark_modified::<T>();
        unsafe { self.value.as_mut() }
    }
}

unsafe impl<'param, 'cached, Q, F> SystemParam for Query<'param, 'cached, Q, F>
where
    Q: QuerySpec + 'static,
    F: QueryFilter + 'static,
{
    type State = QueryState<Q, F>;
    type Item<'world, 'state> = Query<'world, 'state, Q, F>;
    fn init_state(world: &mut World) -> Result<Self::State, SystemParamError> {
        Ok(QueryState::new(world))
    }
    fn access(state: &Self::State) -> QueryAccess {
        state.access().clone()
    }
    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf("query", "Query", std::any::type_name::<Self>())
    }
    unsafe fn extract<'world, 'state>(
        state: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(Query::new(context.query(), state))
    }
}

unsafe impl<'param, 'cached, T: Component + 'static> SystemParam
    for RemovedQuery<'param, 'cached, T>
{
    type State = RemovedState<T>;
    type Item<'world, 'state> = RemovedQuery<'world, 'state, T>;
    fn init_state(world: &mut World) -> Result<Self::State, SystemParamError> {
        Ok(RemovedState::new(world))
    }
    fn access(state: &Self::State) -> QueryAccess {
        state.access().clone()
    }
    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf(
            "query_removed",
            "RemovedQuery",
            std::any::type_name::<Self>(),
        )
    }
    unsafe fn extract<'world, 'state>(
        state: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(RemovedQuery::new(context.query(), state))
    }
}

unsafe impl<'param, T: Resource + 'static> SystemParam for Res<'param, T> {
    type State = ();
    type Item<'world, 'state> = Res<'world, T>;
    fn init_state(world: &mut World) -> Result<Self::State, SystemParamError> {
        world.resource::<T>()?;
        Ok(())
    }
    fn access(_: &Self::State) -> QueryAccess {
        QueryAccess::default().with_resource_read::<T>()
    }
    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf("res", "Res", std::any::type_name::<Self>())
    }
    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(Res::new(context.resource::<T>()?))
    }
}

unsafe impl<'param, T: Resource + 'static> SystemParam for ResMut<'param, T> {
    type State = ();
    type Item<'world, 'state> = ResMut<'world, T>;
    fn init_state(world: &mut World) -> Result<Self::State, SystemParamError> {
        world.resource::<T>()?;
        Ok(())
    }
    fn access(_: &Self::State) -> QueryAccess {
        QueryAccess::default().with_resource_write::<T>()
    }
    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf("res_mut", "ResMut", std::any::type_name::<Self>())
    }
    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(ResMut::new(context.resource_mut::<T>()?))
    }
}

unsafe impl<'param> SystemParam for Commands<'param> {
    type State = ();
    type Item<'world, 'state> = Commands<'world>;
    fn init_state(_: &mut World) -> Result<Self::State, SystemParamError> {
        Ok(())
    }
    fn access(_: &Self::State) -> QueryAccess {
        QueryAccess::structural_mutation()
    }
    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf("commands", "Commands", std::any::type_name::<Self>())
    }
    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(context.commands())
    }
}

macro_rules! impl_tuple_system_param {
    ($(($index:tt, $param:ident)),+ $(,)?) => {
        unsafe impl<$($param: SystemParam),+> SystemParam for ($($param,)+) {
            type State = ($($param::State,)+);
            type Item<'world, 'state> = ($($param::Item<'world, 'state>,)+);
            fn init_state(world: &mut World) -> Result<Self::State, SystemParamError> {
                Ok(($($param::init_state(world)?,)+))
            }
            fn access(state: &Self::State) -> QueryAccess {
                let mut access = QueryAccess::default();
                $(access.extend($param::access(&state.$index));)+
                access
            }
            fn slot_descriptor() -> ParamSlotDescriptor {
                ParamSlotDescriptor::group(
                    "tuple",
                    "Tuple",
                    std::any::type_name::<Self>(),
                    vec![$(ParamSlotDescriptor::named_child(
                        stringify!($index),
                        $param::slot_descriptor(),
                    ),)+],
                )
            }
            unsafe fn extract<'world, 'state>(
                state: &'state mut Self::State,
                context: SystemParamContext<'world>,
            ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
                Ok(($((unsafe { $param::extract(&mut state.$index, context) })?,)+))
            }
        }
    };
}

impl_tuple_system_param!((0, A), (1, B));
impl_tuple_system_param!((0, A), (1, B), (2, C));
impl_tuple_system_param!((0, A), (1, B), (2, C), (3, D));
impl_tuple_system_param!((0, A), (1, B), (2, C), (3, D), (4, E));
impl_tuple_system_param!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F));
impl_tuple_system_param!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G));
impl_tuple_system_param!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H)
);
