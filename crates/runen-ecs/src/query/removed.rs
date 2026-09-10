// Owner: RunenECS - Removed Component Query
use super::access_and_filters::QueryAccess;
use crate::component::Component;
use crate::entity::Entity;
use crate::query::QueryWorldSource;
use crate::world::ChangeCursor;
use crate::world::{QueryCapability, World};
use std::any::TypeId;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::ptr::NonNull;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Removed<T: Component> {
    entity: Entity,
    tick: ChangeCursor,
    _marker: PhantomData<T>,
}

impl<T: Component> Removed<T> {
    pub fn entity(&self) -> Entity {
        self.entity
    }

    pub fn tick(&self) -> ChangeCursor {
        self.tick
    }
}

pub struct RemovedState<T: Component> {
    access: QueryAccess,
    scratch: RefCell<Vec<(Entity, ChangeCursor)>>,
    _marker: PhantomData<T>,
}

impl<T: Component> RemovedState<T> {
    pub fn new(_world: &World) -> Self {
        let mut access = QueryAccess::default();
        access.add_removed_component_read::<T>();
        Self {
            access,
            scratch: RefCell::new(Vec::new()),
            _marker: PhantomData,
        }
    }

    pub fn access(&self) -> &QueryAccess {
        &self.access
    }

    pub fn iter<'w, W>(&self, world: W) -> impl Iterator<Item = Removed<T>> + 'w
    where
        W: QueryWorldSource<'w, Entity>,
    {
        self.iter_capability(world.into_query_capability())
    }

    pub(crate) fn iter_capability<'w>(
        &self,
        world: QueryCapability<'w>,
    ) -> impl Iterator<Item = Removed<T>> + 'w {
        let mut scratch = self.scratch.borrow_mut();
        world.removed_component_records_current_window(TypeId::of::<T>(), &mut scratch);
        let records = scratch
            .iter()
            .map(|(entity, tick)| Removed {
                entity: *entity,
                tick: *tick,
                _marker: PhantomData,
            })
            .collect::<Vec<_>>();
        records.into_iter()
    }
}

pub struct RemovedQuery<'world, 'state, T: Component> {
    world: QueryCapability<'world>,
    state: NonNull<RemovedState<T>>,
    _marker: PhantomData<(&'state mut RemovedState<T>, T)>,
}

impl<'world, 'state, T: Component> RemovedQuery<'world, 'state, T> {
    pub(crate) fn new(world: QueryCapability<'world>, state: &'state mut RemovedState<T>) -> Self {
        Self {
            world,
            state: NonNull::from(state),
            _marker: PhantomData,
        }
    }

    pub fn access(&self) -> &QueryAccess {
        unsafe { self.state.as_ref().access() }
    }

    pub fn iter(&mut self) -> impl Iterator<Item = Removed<T>> + '_ {
        unsafe { self.state.as_ref().iter_capability(self.world) }
    }
}
