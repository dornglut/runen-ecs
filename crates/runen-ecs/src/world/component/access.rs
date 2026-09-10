// Owner: RunenECS World Component - Access, Mutation, Change Tracking, and Matching APIs
use crate::component::Component;
use crate::entity::Entity;
use crate::errors::EntityError;
use crate::world::entity_handles::Mut;
use crate::world::{ChangeCursor, World};
use std::any::{TypeId, type_name};

impl World {
    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        if !self.contains(entity) {
            return None;
        }
        self.archetype_component::<T>(entity)
    }

    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<Mut<'_, T>> {
        if !self.contains(entity) {
            return None;
        }
        let value = self
            .archetype_registry
            .component_mut_ptr::<T>(entity, &self.entity_locations)?;
        self.mark_component_modified_by_id(entity, TypeId::of::<T>());
        // Safety: the typed storage lookup returned a valid mutable component pointer.
        Some(Mut {
            value: unsafe { &mut *value },
        })
    }

    pub fn require<T: Component>(&self, entity: Entity) -> Result<&T, EntityError> {
        self.ensure_entity_exists(entity)?;
        self.archetype_component::<T>(entity)
            .ok_or(EntityError::MissingComponent {
                entity,
                component: type_name::<T>(),
            })
    }

    pub fn require_mut<T: Component>(&mut self, entity: Entity) -> Result<Mut<'_, T>, EntityError> {
        self.ensure_entity_exists(entity)?;
        let value = self
            .archetype_registry
            .component_mut_ptr::<T>(entity, &self.entity_locations)
            .ok_or(EntityError::MissingComponent {
                entity,
                component: type_name::<T>(),
            })?;
        self.mark_component_modified_by_id(entity, TypeId::of::<T>());
        // Safety: the typed storage lookup returned a valid mutable component pointer.
        Ok(Mut {
            value: unsafe { &mut *value },
        })
    }

    pub(crate) fn __commit_insert_component<T: Component>(&mut self, entity: Entity, component: T) {
        debug_assert!(self.contains(entity));
        let already_present = self.contains_component::<T>(entity);
        let component_type = TypeId::of::<T>();
        let commit_tick = self
            .change_tick
            .next()
            .expect("ECS change cursor exhausted");

        let inserted = if !already_present {
            self.archetype_registry.add_component::<T>(
                entity,
                component,
                commit_tick,
                &mut self.entity_locations,
            )
        } else {
            self.archetype_registry.update_component::<T>(
                entity,
                component,
                commit_tick,
                &self.entity_locations,
            )
        };

        assert!(
            inserted,
            "preflighted archetype component insert/update must succeed"
        );
        self.record_component_change(entity, component_type, false);
    }

    pub(crate) fn __commit_remove_component<T: Component>(&mut self, entity: Entity) -> T {
        debug_assert!(self.contains(entity));
        let value = self
            .archetype_registry
            .remove_component::<T>(entity, &mut self.entity_locations)
            .expect("preflighted archetype component removal must succeed");
        self.record_component_change(entity, TypeId::of::<T>(), true);
        value
    }

    pub fn component_changed_since<T: Component>(&self, tick: ChangeCursor) -> bool {
        self.component_change_ticks
            .get(&TypeId::of::<T>())
            .is_some_and(|changed| *changed > tick)
    }

    pub(crate) fn has_component_by_type_id(&self, entity: Entity, type_id: TypeId) -> bool {
        let Some(location) = self.entity_locations.get(entity) else {
            return false;
        };

        self.archetype_registry
            .component_types(location.archetype_id)
            .is_some_and(|component_types| component_types.binary_search(&type_id).is_ok())
    }

    pub(crate) fn contains_component<T: Component>(&self, entity: Entity) -> bool {
        self.has_component_by_type_id(entity, TypeId::of::<T>())
    }

    pub(crate) fn mark_component_modified_by_id(&mut self, entity: Entity, component_type: TypeId) {
        self.record_component_change(entity, component_type, false);

        let _ = self.archetype_registry.mark_component_changed_by_id(
            entity,
            component_type,
            self.change_tick,
            &self.entity_locations,
        );
    }

    fn mark_component_type_changed_by_id(&mut self, type_id: TypeId) {
        self.change_tick = self
            .change_tick
            .next()
            .expect("ECS change cursor exhausted");
        self.component_change_ticks
            .insert(type_id, self.change_tick);
        self.mark_component_indexes_dirty(type_id);
    }

    pub(crate) fn record_component_change(
        &mut self,
        entity: Entity,
        component_type: TypeId,
        removed: bool,
    ) {
        self.mark_component_type_changed_by_id(component_type);
        if removed {
            self.removed_component_records
                .entry(component_type)
                .or_default()
                .push(crate::world::change_tracking::RemovedComponentRecord {
                    tick: self.change_tick,
                    entity,
                });
        }
    }

    pub(crate) fn begin_stage_command_flush(&mut self) {
        self.removed_component_records.clear();
    }

    pub(crate) fn archetype_component<T: Component>(&self, entity: Entity) -> Option<&T> {
        let ptr = self
            .archetype_registry
            .component_ptr::<T>(entity, &self.entity_locations)?;
        Some(unsafe { &*ptr })
    }

    pub(crate) fn archetype_component_metadata<T: Component>(
        &self,
        entity: Entity,
    ) -> Option<(ChangeCursor, ChangeCursor)> {
        let metadata = self
            .archetype_registry
            .component_metadata::<T>(entity, &self.entity_locations)?;
        Some((metadata.added_tick, metadata.changed_tick))
    }

    pub(crate) fn mark_component_indexes_dirty(&mut self, component_type: TypeId) {
        let mut indexes = self.component_indexes.borrow_mut();
        for (index_key, index) in indexes.iter_mut() {
            if index_key.component_type == component_type {
                index.mark_dirty();
            }
        }
    }

    pub(crate) fn matching_entities_into(
        &self,
        required_present: &[TypeId],
        excluded: &[TypeId],
        out: &mut Vec<Entity>,
    ) {
        let _ = self
            .archetype_registry
            .collect_matching_entities(required_present, excluded, out);
    }
}
