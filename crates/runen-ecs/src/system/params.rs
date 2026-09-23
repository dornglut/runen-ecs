use super::extract::{
    DeferredRecorderClass, DeferredRecorderConflict, SystemParam, SystemParamContext,
    SystemParamError, TransferableSystemParam, WorkerPrepareContext,
};
use crate::World;
use crate::component::{Component, Resource};
use crate::query::{
    Query, QueryAccess, QueryFilter, QuerySpec, QueryState, RemovedQuery, RemovedState,
    TransferableQueryData, TransferableQueryFilter, prepare_query,
};
use crate::scheduler::system::ParamSlotDescriptor;
use crate::world::{ResourceCapability, ResourceMutationCapability};
use crate::{Commands, LocalCommands, Relation, Relations, RelationsMut};
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

    fn init_state() -> Result<Self::State, SystemParamError> {
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
    fn init_state() -> Result<Self::State, SystemParamError> {
        QueryState::unbound().map_err(SystemParamError::from)
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

unsafe impl<'param, 'cached, Q, F> TransferableSystemParam for Query<'param, 'cached, Q, F>
where
    Q: QuerySpec + TransferableQueryData + 'static,
    F: QueryFilter + TransferableQueryFilter + 'static,
{
    fn prepare_worker(
        _state: &Self::State,
        context: &mut WorkerPrepareContext<'_>,
    ) -> Result<(), SystemParamError> {
        prepare_query::<Q, F>(context.builder());
        Ok(())
    }
}

unsafe impl<'param, 'cached, T: Component + 'static> SystemParam
    for RemovedQuery<'param, 'cached, T>
{
    type State = RemovedState<T>;
    type Item<'world, 'state> = RemovedQuery<'world, 'state, T>;
    fn init_state() -> Result<Self::State, SystemParamError> {
        Ok(RemovedState::unbound())
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

unsafe impl<'param, 'cached, T: Component + 'static> TransferableSystemParam
    for RemovedQuery<'param, 'cached, T>
{
    fn prepare_worker(
        _state: &Self::State,
        context: &mut WorkerPrepareContext<'_>,
    ) -> Result<(), SystemParamError> {
        context.builder().prepare_removed::<T>();
        Ok(())
    }
}

unsafe impl<'param, T: Resource + 'static> SystemParam for Res<'param, T> {
    type State = ();
    type Item<'world, 'state> = Res<'world, T>;
    fn init_state() -> Result<Self::State, SystemParamError> {
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

unsafe impl<'param, T: Resource + Sync + 'static> TransferableSystemParam for Res<'param, T> {
    fn prepare_worker(
        _state: &Self::State,
        context: &mut WorkerPrepareContext<'_>,
    ) -> Result<(), SystemParamError> {
        context.builder().prepare_resource_read::<T>()?;
        Ok(())
    }
}

unsafe impl<'param, T: Resource + 'static> SystemParam for ResMut<'param, T> {
    type State = ();
    type Item<'world, 'state> = ResMut<'world, T>;
    fn init_state() -> Result<Self::State, SystemParamError> {
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

unsafe impl<'param, T: Resource + Send + 'static> TransferableSystemParam for ResMut<'param, T> {
    fn prepare_worker(
        _state: &Self::State,
        context: &mut WorkerPrepareContext<'_>,
    ) -> Result<(), SystemParamError> {
        context.builder().prepare_resource_write::<T>()?;
        Ok(())
    }
}

unsafe impl<'param, R: Relation> SystemParam for Relations<'param, R> {
    type State = ();
    type Item<'world, 'state> = Relations<'world, R>;

    fn init_state() -> Result<Self::State, SystemParamError> {
        Ok(())
    }

    fn access(_: &Self::State) -> QueryAccess {
        QueryAccess::default().with_relation_read::<R>()
    }

    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf("relations", "Relations", std::any::type_name::<Self>())
    }

    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(Relations::from_capability(context.relation::<R>()))
    }
}

unsafe impl<'param, R: Relation> TransferableSystemParam for Relations<'param, R> {
    fn prepare_worker(
        _state: &Self::State,
        context: &mut WorkerPrepareContext<'_>,
    ) -> Result<(), SystemParamError> {
        context.builder().prepare_relation_read::<R>();
        Ok(())
    }
}

unsafe impl<'param, R: Relation> SystemParam for RelationsMut<'param, R> {
    type State = ();
    type Item<'world, 'state> = RelationsMut<'world, R>;

    fn init_state() -> Result<Self::State, SystemParamError> {
        Ok(())
    }

    fn access(_: &Self::State) -> QueryAccess {
        QueryAccess::default().with_relation_write::<R>()
    }

    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf(
            "relations_mut",
            "RelationsMut",
            std::any::type_name::<Self>(),
        )
    }

    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(RelationsMut::from_capability(context.relation_mut::<R>()))
    }
}

unsafe impl<'param, R: Relation> TransferableSystemParam for RelationsMut<'param, R> {
    fn prepare_worker(
        _state: &Self::State,
        context: &mut WorkerPrepareContext<'_>,
    ) -> Result<(), SystemParamError> {
        context.builder().prepare_relation_write::<R>();
        Ok(())
    }
}

unsafe impl<'param> SystemParam for LocalCommands<'param> {
    type State = ();
    type Item<'world, 'state> = LocalCommands<'world>;
    fn init_state() -> Result<Self::State, SystemParamError> {
        Ok(())
    }
    fn deferred_recorder_class() -> Result<DeferredRecorderClass, DeferredRecorderConflict> {
        Ok(DeferredRecorderClass::LocalDeferred)
    }
    fn access(_: &Self::State) -> QueryAccess {
        QueryAccess::structural_mutation()
    }
    fn slot_descriptor() -> ParamSlotDescriptor {
        ParamSlotDescriptor::leaf(
            "local_commands",
            "LocalCommands",
            std::any::type_name::<Self>(),
        )
    }
    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(context.local_commands())
    }
}

unsafe impl<'param> SystemParam for Commands<'param> {
    type State = ();
    type Item<'world, 'state> = Commands<'world>;

    fn init_state() -> Result<Self::State, SystemParamError> {
        Ok(())
    }

    fn deferred_recorder_class() -> Result<DeferredRecorderClass, DeferredRecorderConflict> {
        Ok(DeferredRecorderClass::TransferableDeferred)
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

unsafe impl<'param> TransferableSystemParam for Commands<'param> {
    fn prepare_worker(
        _state: &Self::State,
        _context: &mut WorkerPrepareContext<'_>,
    ) -> Result<(), SystemParamError> {
        Ok(())
    }
}

macro_rules! impl_tuple_system_param {
    ($(($index:tt, $param:ident)),+ $(,)?) => {
        unsafe impl<$($param: SystemParam),+> SystemParam for ($($param,)+) {
            type State = ($($param::State,)+);
            type Item<'world, 'state> = ($($param::Item<'world, 'state>,)+);
            fn init_state() -> Result<Self::State, SystemParamError> {
                Ok(($($param::init_state()?,)+))
            }
            fn deferred_recorder_class() -> Result<DeferredRecorderClass, DeferredRecorderConflict> {
                let mut class = DeferredRecorderClass::None;
                $(class = class.merge($param::deferred_recorder_class()?)?;)+
                Ok(class)
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

        unsafe impl<$($param: TransferableSystemParam),+> TransferableSystemParam
            for ($($param,)+)
        where
            $($param::State: Send,)+
        {
            fn prepare_worker(
                state: &Self::State,
                context: &mut WorkerPrepareContext<'_>,
            ) -> Result<(), SystemParamError> {
                $(
                    $param::prepare_worker(&state.$index, context)?;
                )+
                Ok(())
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
