// Owner: RunenECS World Entity - Lifecycle APIs
use crate::bundle::{Bundle, prepare_bundle};
use crate::entity::Entity;
use crate::errors::{EntityAllocationError, EntityError};
use crate::world::World;

impl World {
    pub fn contains(&self, entity: Entity) -> bool {
        self.allocator.contains(entity) && self.alive_entities.contains(&entity)
    }

    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Result<Entity, EntityAllocationError> {
        let prepared = prepare_bundle(bundle);
        let entity = self.allocator.allocate()?;

        self.register_bundle_descriptors(prepared.descriptors());
        self.alive_entities.insert(entity);
        self.place_entity_in_empty_archetype(entity);
        for component in prepared.into_components() {
            component.commit_insert(self, entity);
        }

        Ok(entity)
    }

    pub fn despawn(&mut self, entity: Entity) -> Result<(), EntityError> {
        self.ensure_entity_exists(entity)?;

        let removed_types = self
            .entity_locations
            .get(entity)
            .and_then(|location| {
                self.archetype_registry
                    .component_types(location.archetype_id)
                    .map(|types| types.to_vec())
            })
            .unwrap_or_default();

        self.remove_entity_from_archetype_tracking(entity);
        self.alive_entities.remove(&entity);
        self.allocator.free(entity)?;

        for type_id in removed_types {
            self.record_component_change(entity, type_id, true);
        }

        Ok(())
    }

    pub(crate) fn place_entity_in_empty_archetype(&mut self, entity: Entity) {
        self.archetype_registry
            .set_entity_components(entity, &[], &mut self.entity_locations);
    }

    pub(crate) fn remove_entity_from_archetype_tracking(&mut self, entity: Entity) {
        let _ = self
            .archetype_registry
            .remove_entity(entity, &mut self.entity_locations);
    }

    pub(crate) fn ensure_entity_exists(&self, entity: Entity) -> Result<(), EntityError> {
        self.allocator.validate(entity)?;
        if self.alive_entities.contains(&entity) {
            Ok(())
        } else {
            Err(EntityError::UnknownEntity { entity })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    #[derive(crate::Component)]
    struct Marker;

    #[derive(Debug, PartialEq, Eq, crate::Component)]
    struct Value(u32);

    #[derive(crate::Component)]
    struct NeverRegistered;

    #[test]
    fn failed_spawn_allocation_has_no_structural_registration_side_effects() {
        let mut world = World::new();
        world.allocator.exhaust_index_space_for_test();
        let alive_before = world.alive_entities.len();
        let changes_before = world.component_change_ticks.len();

        assert_eq!(
            world.spawn(NeverRegistered),
            Err(EntityAllocationError::IndexExhausted)
        );
        assert_eq!(world.alive_entities.len(), alive_before);
        assert_eq!(world.component_change_ticks.len(), changes_before);
        assert!(!world.has_component_type(TypeId::of::<NeverRegistered>()));
        assert!(world.entity_locations.is_empty());
    }

    #[test]
    fn successful_spawn_makes_entity_live() {
        let mut world = World::new();
        let entity = world.spawn(Marker).expect("spawn should succeed");

        assert!(world.contains(entity));
    }

    #[test]
    fn fresh_worlds_never_alias_equal_local_entity_positions() {
        let mut first_world = World::new();
        let mut second_world = World::new();
        let first = first_world
            .spawn(Marker)
            .expect("first spawn should succeed");
        let second = second_world
            .spawn(Marker)
            .expect("second spawn should succeed");

        assert_eq!(first.index(), second.index());
        assert_eq!(first.generation(), second.generation());
        assert_ne!(first, second);
        assert!(!first_world.contains(second));
        assert!(matches!(
            first_world.entity(second),
            Err(EntityError::ForeignWorld { .. })
        ));
        assert!(matches!(
            first_world.require::<Marker>(second),
            Err(EntityError::ForeignWorld { .. })
        ));
        assert!(matches!(
            first_world.despawn(second),
            Err(EntityError::ForeignWorld { .. })
        ));
        assert!(first_world.contains(first));
        assert!(second_world.contains(second));
    }

    #[test]
    fn stale_and_double_free_entities_are_rejected_before_world_mutation() {
        let mut world = World::new();
        let first = world.spawn(Value(1)).expect("spawn should succeed");
        world.despawn(first).expect("despawn should succeed");
        assert!(matches!(
            world.despawn(first),
            Err(EntityError::AlreadyFreed { .. })
        ));

        let second = world.spawn(Value(7)).expect("reuse should succeed");
        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());

        let third = world.spawn(Value(9)).expect("fresh slot should allocate");
        assert_ne!(
            third.index(),
            first.index(),
            "rejected double free must not duplicate the reusable slot"
        );

        assert!(world.get::<Value>(first).is_none());
        assert!(world.get_mut::<Value>(first).is_none());
        assert!(matches!(
            world.entity(first),
            Err(EntityError::StaleGeneration { .. })
        ));
        assert!(matches!(
            world.require_mut::<Value>(first),
            Err(EntityError::StaleGeneration { .. })
        ));
        assert!(matches!(
            world.insert(first, Value(99)),
            Err(EntityError::StaleGeneration { .. })
        ));
        assert!(matches!(
            world.remove::<Value>(first),
            Err(EntityError::StaleGeneration { .. })
        ));
        assert!(matches!(
            world.despawn(first),
            Err(EntityError::StaleGeneration { .. })
        ));

        assert!(world.contains(second));
        assert!(world.contains(third));
        assert_eq!(
            world
                .require::<Value>(second)
                .expect("newer entity must remain valid")
                .0,
            7
        );
        assert_eq!(
            world
                .require::<Value>(third)
                .expect("independent entity must remain valid")
                .0,
            9
        );
    }
}
