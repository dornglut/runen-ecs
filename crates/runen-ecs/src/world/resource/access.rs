// Owner: RunenECS World Resource - Resource Access APIs
use crate::component::Resource;
use crate::errors::ResourceError;
use crate::world::{ChangeCursor, World};
use std::any::{Any, TypeId, type_name};

impl World {
    pub fn insert_resource<R: Resource>(&mut self, resource: R) {
        let type_id = TypeId::of::<R>();
        self.resources.insert(type_id, Box::new(resource));
        self.record_resource_change(type_id);
    }

    pub fn has_resource<R: Resource>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<R>())
    }

    pub fn resource<R: Resource>(&self) -> Result<&R, ResourceError> {
        self.resources
            .get(&TypeId::of::<R>())
            .and_then(|res| res.downcast_ref::<R>())
            .ok_or(ResourceError::Missing {
                resource: type_name::<R>(),
            })
    }

    pub fn resource_by_type_id(&self, type_id: TypeId) -> Option<&dyn Any> {
        self.resources
            .get(&type_id)
            .map(|resource| resource.as_ref())
    }

    pub fn resource_mut<R: Resource>(&mut self) -> Result<&mut R, ResourceError> {
        let type_id = TypeId::of::<R>();
        if !self.resources.contains_key(&type_id) {
            return Err(ResourceError::Missing {
                resource: type_name::<R>(),
            });
        }

        let value = self
            .resources
            .get_mut(&type_id)
            .and_then(|res| res.downcast_mut::<R>())
            .map(|value| value as *mut R)
            .ok_or(ResourceError::Missing {
                resource: type_name::<R>(),
            })?;

        self.record_resource_change(type_id);

        // Safety: the pointer came from the successful unique lookup above;
        // recording the change does not move the boxed resource allocation.
        Ok(unsafe { &mut *value })
    }

    pub fn remove_resource<R: Resource>(&mut self) -> Option<R> {
        let type_id = TypeId::of::<R>();
        let removed = self
            .resources
            .remove(&type_id)
            .and_then(|res| res.downcast::<R>().ok().map(|boxed| *boxed));

        if removed.is_some() {
            self.record_resource_change(type_id);
        }

        removed
    }

    pub fn resource_changed_since<R: Resource>(&self, tick: ChangeCursor) -> bool {
        self.resource_change_ticks
            .get(&TypeId::of::<R>())
            .is_some_and(|changed| *changed > tick)
    }

    pub(crate) fn record_resource_change(&mut self, resource_type: TypeId) {
        self.change_tick = self
            .change_tick
            .next()
            .expect("ECS change cursor exhausted");
        self.resource_change_ticks
            .insert(resource_type, self.change_tick);
    }
}
