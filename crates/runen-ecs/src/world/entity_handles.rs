// Owner: RunenECS World - Borrow Wrappers and Entity Views
use super::World;
use crate::bundle::Bundle;
use crate::component::Component;
use crate::entity::Entity;
use crate::errors::EntityError;
use std::ops::{Deref, DerefMut};

pub struct Mut<'a, T> {
    pub(super) value: &'a mut T,
}

impl<'a, T> Deref for Mut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'a, T> DerefMut for Mut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<'a, T> Mut<'a, T> {
    pub fn into_inner(self) -> &'a mut T {
        self.value
    }
}

pub struct EntityRef<'w> {
    pub(super) world: &'w World,
    pub(super) entity: Entity,
}

impl<'w> EntityRef<'w> {
    pub fn entity(&self) -> Entity {
        self.entity
    }

    pub fn contains<T: Component>(&self) -> bool {
        self.world.contains_component::<T>(self.entity)
    }

    pub fn get<T: Component>(&self) -> Option<&T> {
        self.world.get::<T>(self.entity)
    }

    pub fn require<T: Component>(&self) -> Result<&T, EntityError> {
        self.world.require::<T>(self.entity)
    }
}

pub struct EntityMut<'w> {
    pub(super) world: &'w mut World,
    pub(super) entity: Entity,
}

impl<'w> EntityMut<'w> {
    pub fn entity(&self) -> Entity {
        self.entity
    }

    pub fn contains<T: Component>(&self) -> bool {
        self.world.contains_component::<T>(self.entity)
    }

    pub fn get<T: Component>(&self) -> Option<&T> {
        self.world.get::<T>(self.entity)
    }

    pub fn get_mut<T: Component>(&mut self) -> Option<Mut<'_, T>> {
        self.world.get_mut::<T>(self.entity)
    }

    pub fn require<T: Component>(&self) -> Result<&T, EntityError> {
        self.world.require::<T>(self.entity)
    }

    pub fn require_mut<T: Component>(&mut self) -> Result<Mut<'_, T>, EntityError> {
        self.world.require_mut::<T>(self.entity)
    }

    pub fn insert<B: Bundle>(&mut self, bundle: B) -> Result<(), EntityError> {
        self.world.insert(self.entity, bundle)
    }

    pub fn remove<B: Bundle>(&mut self) -> Result<B, EntityError> {
        self.world.remove::<B>(self.entity)
    }

    pub fn despawn(self) -> Result<(), EntityError> {
        self.world.despawn(self.entity)
    }
}
