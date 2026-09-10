// Owner: RunenECS World Entity - Access APIs
use crate::bundle::{
    Bundle, BundleComponentValue, BundleComponents, bundle_descriptors, prepare_bundle,
};
use crate::component::Component;
use crate::entity::Entity;
use crate::errors::EntityError;
use crate::world::entity_handles::{EntityMut, EntityRef};
use crate::world::{ChangeCursor, World};
use std::collections::HashSet;

impl World {
    pub fn insert<B: Bundle>(&mut self, entity: Entity, bundle: B) -> Result<(), EntityError> {
        self.ensure_entity_exists(entity)?;
        let prepared = prepare_bundle(bundle);
        self.register_bundle_descriptors(prepared.descriptors());
        for component in prepared.into_components() {
            component.commit_insert(self, entity);
        }
        Ok(())
    }

    pub fn remove<B: Bundle>(&mut self, entity: Entity) -> Result<B, EntityError> {
        self.ensure_entity_exists(entity)?;
        let descriptors = bundle_descriptors::<B>();
        let mut removed_types = HashSet::with_capacity(descriptors.len());

        for descriptor in &descriptors {
            if !removed_types.insert(descriptor.component_type_id())
                || !self.has_component_by_type_id(entity, descriptor.component_type_id())
            {
                return Err(EntityError::MissingComponent {
                    entity,
                    component: descriptor.component_name(),
                });
            }
        }

        let mut values = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            values.push(BundleComponentValue::from_removed(descriptor, self, entity));
        }
        let mut components = BundleComponents::from_values(values);
        // Safety: removal preflight and the unsafe Bundle implementation contract
        // guarantee that the collected values match B's declared descriptors.
        Ok(unsafe { B::__from_components(&mut components) })
    }

    pub fn entity(&self, entity: Entity) -> Result<EntityRef<'_>, EntityError> {
        self.ensure_entity_exists(entity)?;
        Ok(EntityRef {
            world: self,
            entity,
        })
    }

    pub fn entity_mut(&mut self, entity: Entity) -> Result<EntityMut<'_>, EntityError> {
        self.ensure_entity_exists(entity)?;
        Ok(EntityMut {
            world: self,
            entity,
        })
    }

    #[doc(hidden)]
    pub fn __entity_archetype_location(&self, entity: Entity) -> Option<(usize, usize)> {
        self.entity_locations
            .get(entity)
            .map(|location| (location.archetype_id.index(), location.row))
    }

    #[doc(hidden)]
    pub fn __entity_archetype_component_count(&self, entity: Entity) -> Option<usize> {
        let location = self.entity_locations.get(entity)?;
        self.archetype_registry
            .component_count(location.archetype_id)
    }

    #[doc(hidden)]
    pub fn __entity_component_ticks<T: Component>(
        &self,
        entity: Entity,
    ) -> Option<(ChangeCursor, ChangeCursor)> {
        self.archetype_component_metadata::<T>(entity)
    }
}
