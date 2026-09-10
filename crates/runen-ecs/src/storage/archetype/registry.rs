// Owner: RunenECS Storage - Archetype Registry, Matching, and Typed Column Storage
use super::{EntityLocation, EntityLocationMap};
use crate::component::Component;
use crate::entity::Entity;
use crate::storage::dense::{DenseColumn, DenseEntityColumn, DenseRowMetadata};
use crate::world::ChangeCursor;
use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct ArchetypeExecutionBinding {
    pub(crate) archetype_index: usize,
    pub(crate) row_count: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ArchetypeId(usize);

impl ArchetypeId {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ArchetypeKey {
    component_types: Vec<TypeId>,
}

impl ArchetypeKey {
    fn new(component_types: &[TypeId]) -> Self {
        let mut canonical = component_types.to_vec();
        canonical.sort_unstable();
        canonical.dedup();
        Self {
            component_types: canonical,
        }
    }

    fn component_types(&self) -> &[TypeId] {
        &self.component_types
    }

    fn contains(&self, type_id: TypeId) -> bool {
        self.component_types.binary_search(&type_id).is_ok()
    }
}

struct ErasedDenseRow {
    value: Box<dyn Any>,
    metadata: DenseRowMetadata,
}

impl ErasedDenseRow {
    fn new_added<T: Component>(value: T, tick: ChangeCursor) -> Self {
        Self {
            value: Box::new(Box::new(value)),
            metadata: DenseRowMetadata::new(tick, tick),
        }
    }

    fn into_typed_value<T: Component>(self) -> T {
        let boxed = self
            .value
            .downcast::<Box<T>>()
            .expect("archetype row value type mismatch");
        **boxed
    }
}

trait ArchetypeComponentColumn {
    fn len(&self) -> usize;
    fn get_ptr(&self, row: usize) -> Option<*const ()>;
    fn get_mut_ptr(&mut self, row: usize) -> Option<*mut ()>;
    fn metadata(&self, row: usize) -> Option<DenseRowMetadata>;
    fn mark_changed(&mut self, row: usize, tick: ChangeCursor) -> bool;
    fn push_erased(&mut self, row: ErasedDenseRow);
    fn swap_remove_erased(&mut self, row: usize) -> Option<ErasedDenseRow>;
}

struct TypedArchetypeColumn<T: Component> {
    dense: DenseColumn<T>,
}

impl<T: Component> Default for TypedArchetypeColumn<T> {
    fn default() -> Self {
        Self {
            dense: DenseColumn::default(),
        }
    }
}

impl<T: Component> ArchetypeComponentColumn for TypedArchetypeColumn<T> {
    fn len(&self) -> usize {
        self.dense.len()
    }

    fn get_ptr(&self, row: usize) -> Option<*const ()> {
        self.dense.get_ptr(row).map(|ptr| ptr.cast::<()>())
    }

    fn get_mut_ptr(&mut self, row: usize) -> Option<*mut ()> {
        self.dense.get_mut_ptr(row).map(|ptr| ptr.cast::<()>())
    }

    fn metadata(&self, row: usize) -> Option<DenseRowMetadata> {
        self.dense.metadata(row)
    }

    fn mark_changed(&mut self, row: usize, tick: ChangeCursor) -> bool {
        self.dense.mark_changed(row, tick)
    }

    fn push_erased(&mut self, row: ErasedDenseRow) {
        let value = row
            .value
            .downcast::<Box<T>>()
            .expect("archetype row value type mismatch");
        let _ = self.dense.push_boxed(*value, row.metadata);
    }

    fn swap_remove_erased(&mut self, row: usize) -> Option<ErasedDenseRow> {
        let removed = self.dense.swap_remove_boxed(row)?;
        Some(ErasedDenseRow {
            value: Box::new(removed.removed_value),
            metadata: removed.removed_metadata,
        })
    }
}

type ColumnFactory = fn() -> Box<dyn ArchetypeComponentColumn>;

fn typed_column_factory<T: Component>() -> Box<dyn ArchetypeComponentColumn> {
    Box::new(TypedArchetypeColumn::<T>::default())
}

struct ArchetypeRecord {
    key: ArchetypeKey,
    entities: DenseEntityColumn,
    columns: HashMap<TypeId, Box<dyn ArchetypeComponentColumn>>,
}

impl ArchetypeRecord {
    fn new(key: ArchetypeKey) -> Self {
        Self {
            key,
            entities: DenseEntityColumn::default(),
            columns: HashMap::new(),
        }
    }

    fn matches_constraints(&self, required: &[TypeId], excluded: &[TypeId]) -> bool {
        required.iter().all(|type_id| self.key.contains(*type_id))
            && excluded.iter().all(|type_id| !self.key.contains(*type_id))
    }
}

#[derive(Default)]
pub(crate) struct ArchetypeRegistry {
    archetypes: Vec<ArchetypeRecord>,
    key_to_id: HashMap<ArchetypeKey, ArchetypeId>,
    column_factories: HashMap<TypeId, ColumnFactory>,
}

impl ArchetypeRegistry {
    pub(crate) fn new() -> Self {
        let mut registry = Self::default();
        let empty = ArchetypeKey::new(&[]);
        let empty_id = ArchetypeId::new(0);
        registry.key_to_id.insert(empty.clone(), empty_id);
        registry.archetypes.push(ArchetypeRecord::new(empty));
        registry
    }

    pub(crate) fn register_component_type<T: Component>(&mut self) {
        self.column_factories
            .entry(TypeId::of::<T>())
            .or_insert(typed_column_factory::<T>);
    }

    pub(crate) fn component_types(&self, archetype_id: ArchetypeId) -> Option<&[TypeId]> {
        self.archetypes
            .get(archetype_id.index())
            .map(|archetype| archetype.key.component_types())
    }

    pub(crate) fn component_count(&self, archetype_id: ArchetypeId) -> Option<usize> {
        self.component_types(archetype_id).map(|types| types.len())
    }

    pub(crate) fn entity_at(&self, archetype_index: usize, row: usize) -> Option<Entity> {
        self.archetypes.get(archetype_index)?.entities.get(row)
    }

    pub(crate) fn component_ptr<T: Component>(
        &self,
        entity: Entity,
        locations: &EntityLocationMap,
    ) -> Option<*const T> {
        let location = locations.get(entity)?;
        let type_id = TypeId::of::<T>();
        let archetype = self.archetypes.get(location.archetype_id.index())?;
        if !archetype.key.contains(type_id) {
            return None;
        }
        let ptr = archetype.columns.get(&type_id)?.get_ptr(location.row)?;
        Some(ptr.cast::<T>())
    }

    pub(crate) fn component_mut_ptr<T: Component>(
        &mut self,
        entity: Entity,
        locations: &EntityLocationMap,
    ) -> Option<*mut T> {
        let location = locations.get(entity)?;
        let type_id = TypeId::of::<T>();
        let archetype = self.archetypes.get_mut(location.archetype_id.index())?;
        if !archetype.key.contains(type_id) {
            return None;
        }
        let ptr = archetype
            .columns
            .get_mut(&type_id)?
            .get_mut_ptr(location.row)?;
        Some(ptr.cast::<T>())
    }

    pub(crate) fn component_metadata<T: Component>(
        &self,
        entity: Entity,
        locations: &EntityLocationMap,
    ) -> Option<DenseRowMetadata> {
        let location = locations.get(entity)?;
        let type_id = TypeId::of::<T>();
        let archetype = self.archetypes.get(location.archetype_id.index())?;
        if !archetype.key.contains(type_id) {
            return None;
        }
        archetype.columns.get(&type_id)?.metadata(location.row)
    }

    pub(crate) fn mark_component_changed_by_id(
        &mut self,
        entity: Entity,
        component_type: TypeId,
        tick: ChangeCursor,
        locations: &EntityLocationMap,
    ) -> bool {
        let Some(location) = locations.get(entity) else {
            return false;
        };
        let Some(archetype) = self.archetypes.get_mut(location.archetype_id.index()) else {
            return false;
        };
        if !archetype.key.contains(component_type) {
            return false;
        }
        archetype
            .columns
            .get_mut(&component_type)
            .is_some_and(|column| column.mark_changed(location.row, tick))
    }

    pub(crate) fn add_component<T: Component>(
        &mut self,
        entity: Entity,
        value: T,
        tick: ChangeCursor,
        locations: &mut EntityLocationMap,
    ) -> bool {
        let Some(current) = locations.get(entity) else {
            return false;
        };
        let type_id = TypeId::of::<T>();
        let source_types = self
            .component_types(current.archetype_id)
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        if source_types.binary_search(&type_id).is_ok() {
            return self.update_component::<T>(entity, value, tick, locations);
        }

        let mut target_types = source_types.clone();
        insert_sorted_type(&mut target_types, type_id);
        let target_id = self.find_or_create(&target_types);
        let mut moved_rows = self.extract_rows(current, &source_types, locations);
        moved_rows.insert(type_id, ErasedDenseRow::new_added(value, tick));
        self.append_entity_row(entity, target_id, &target_types, moved_rows, locations);
        true
    }

    pub(crate) fn update_component<T: Component>(
        &mut self,
        entity: Entity,
        value: T,
        tick: ChangeCursor,
        locations: &EntityLocationMap,
    ) -> bool {
        let Some(location) = locations.get(entity) else {
            return false;
        };
        let type_id = TypeId::of::<T>();
        let Some(archetype) = self.archetypes.get_mut(location.archetype_id.index()) else {
            return false;
        };
        if !archetype.key.contains(type_id) {
            return false;
        }
        let Some(ptr) = archetype
            .columns
            .get_mut(&type_id)
            .and_then(|column| column.get_mut_ptr(location.row))
        else {
            return false;
        };
        // Safety: type id ensures `ptr` points to `T` in the requested column row.
        unsafe { *(ptr.cast::<T>()) = value };
        archetype
            .columns
            .get_mut(&type_id)
            .is_some_and(|column| column.mark_changed(location.row, tick))
    }

    pub(crate) fn remove_component<T: Component>(
        &mut self,
        entity: Entity,
        locations: &mut EntityLocationMap,
    ) -> Option<T> {
        let current = locations.get(entity)?;
        let type_id = TypeId::of::<T>();
        let source_types = self
            .component_types(current.archetype_id)
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        if source_types.binary_search(&type_id).is_err() {
            return None;
        }

        let mut target_types = source_types.clone();
        target_types.retain(|existing| *existing != type_id);
        let target_id = self.find_or_create(&target_types);
        let mut moved_rows = self.extract_rows(current, &source_types, locations);
        let removed = moved_rows.remove(&type_id)?;
        self.append_entity_row(entity, target_id, &target_types, moved_rows, locations);
        Some(removed.into_typed_value::<T>())
    }

    pub(crate) fn collect_matching_bindings(
        &self,
        required_present: &[TypeId],
        excluded: &[TypeId],
        out: &mut Vec<ArchetypeExecutionBinding>,
    ) -> bool {
        out.clear();
        for (index, archetype) in self.archetypes.iter().enumerate() {
            if !archetype.matches_constraints(required_present, excluded) {
                continue;
            }
            let row_count = archetype.entities.len();
            if row_count == 0 {
                continue;
            }
            out.push(ArchetypeExecutionBinding {
                archetype_index: index,
                row_count,
            });
        }
        true
    }

    pub(crate) fn set_entity_components(
        &mut self,
        entity: Entity,
        component_types: &[TypeId],
        locations: &mut EntityLocationMap,
    ) -> EntityLocation {
        let target_id = self.find_or_create(component_types);
        if let Some(current) = locations.get(entity) {
            if current.archetype_id == target_id {
                return current;
            }
            self.remove_row(current, locations);
        }

        let target = self
            .archetypes
            .get_mut(target_id.index())
            .expect("target archetype must exist");
        let row = target.entities.push(entity);
        let location = EntityLocation {
            archetype_id: target_id,
            row,
        };
        locations.insert(entity, location);
        location
    }

    pub(crate) fn remove_entity(
        &mut self,
        entity: Entity,
        locations: &mut EntityLocationMap,
    ) -> bool {
        let Some(current) = locations.get(entity) else {
            return false;
        };
        let source_types = self
            .component_types(current.archetype_id)
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        let _ = self.extract_rows(current, &source_types, locations);
        let _ = locations.remove(entity);
        true
    }

    pub(crate) fn collect_matching_entities(
        &self,
        required_present: &[TypeId],
        excluded: &[TypeId],
        out: &mut Vec<Entity>,
    ) -> bool {
        let mut bindings = Vec::new();
        let _ = self.collect_matching_bindings(required_present, excluded, &mut bindings);

        out.clear();
        for binding in &bindings {
            for row in 0..binding.row_count {
                if let Some(entity) = self.entity_at(binding.archetype_index, row) {
                    out.push(entity);
                }
            }
        }
        out.sort_unstable();
        true
    }

    fn find_or_create(&mut self, component_types: &[TypeId]) -> ArchetypeId {
        let key = ArchetypeKey::new(component_types);
        if let Some(existing) = self.key_to_id.get(&key).copied() {
            return existing;
        }

        let id = ArchetypeId::new(self.archetypes.len());
        self.key_to_id.insert(key.clone(), id);
        self.archetypes.push(ArchetypeRecord::new(key));
        id
    }

    fn ensure_column_for_type(&mut self, archetype_id: ArchetypeId, type_id: TypeId) {
        let Some(archetype) = self.archetypes.get_mut(archetype_id.index()) else {
            return;
        };
        if archetype.columns.contains_key(&type_id) {
            return;
        }
        assert!(
            archetype.entities.is_empty(),
            "cannot create missing archetype column with existing rows"
        );
        let factory = self
            .column_factories
            .get(&type_id)
            .expect("component type must be registered before archetype value storage");
        archetype.columns.insert(type_id, factory());
    }

    fn extract_rows(
        &mut self,
        location: EntityLocation,
        source_types: &[TypeId],
        locations: &mut EntityLocationMap,
    ) -> HashMap<TypeId, ErasedDenseRow> {
        let Some(archetype) = self.archetypes.get_mut(location.archetype_id.index()) else {
            return HashMap::new();
        };
        let Some(swap_remove) = archetype.entities.swap_remove(location.row) else {
            return HashMap::new();
        };
        if let Some(moved_entity) = swap_remove.moved_entity {
            locations.insert(
                moved_entity,
                EntityLocation {
                    archetype_id: location.archetype_id,
                    row: location.row,
                },
            );
        }

        let mut rows = HashMap::new();
        for type_id in source_types {
            if let Some(column) = archetype.columns.get_mut(type_id) {
                let removed = column
                    .swap_remove_erased(location.row)
                    .expect("source archetype column row must exist");
                rows.insert(*type_id, removed);
            }
        }
        rows
    }

    fn append_entity_row(
        &mut self,
        entity: Entity,
        archetype_id: ArchetypeId,
        target_types: &[TypeId],
        mut moved_rows: HashMap<TypeId, ErasedDenseRow>,
        locations: &mut EntityLocationMap,
    ) -> EntityLocation {
        for type_id in target_types {
            self.ensure_column_for_type(archetype_id, *type_id);
        }

        let target = self
            .archetypes
            .get_mut(archetype_id.index())
            .expect("target archetype must exist");
        let row = target.entities.push(entity);
        for type_id in target_types {
            let entry = moved_rows
                .remove(type_id)
                .expect("target archetype row value must exist");
            let column = target
                .columns
                .get_mut(type_id)
                .expect("target archetype column must exist");
            assert_eq!(
                column.len(),
                row,
                "target column row alignment must match entity row"
            );
            column.push_erased(entry);
        }

        let location = EntityLocation { archetype_id, row };
        locations.insert(entity, location);
        location
    }

    fn remove_row(&mut self, location: EntityLocation, locations: &mut EntityLocationMap) {
        let source_types = self
            .component_types(location.archetype_id)
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        let _ = self.extract_rows(location, &source_types, locations);
    }
}

fn insert_sorted_type(component_types: &mut Vec<TypeId>, type_id: TypeId) {
    match component_types.binary_search(&type_id) {
        Ok(_) => {}
        Err(index) => component_types.insert(index, type_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_entity_components_moves_entity_between_archetypes_and_updates_swapped_location() {
        let mut registry = ArchetypeRegistry::new();
        let mut locations = EntityLocationMap::default();

        let t1 = TypeId::of::<u32>();
        let t2 = TypeId::of::<u64>();

        let first = Entity::test_only(1, 0);
        let second = Entity::test_only(2, 0);

        registry.set_entity_components(first, &[t1], &mut locations);
        registry.set_entity_components(second, &[t1], &mut locations);
        let second_location = locations.get(second).expect("entity must have location");
        assert_eq!(second_location.row, 1);

        registry.set_entity_components(first, &[t1, t2], &mut locations);
        let moved_second = locations
            .get(second)
            .expect("swapped entity should remain mapped");
        assert_eq!(moved_second.row, 0);
    }

    #[test]
    fn remove_entity_updates_swapped_row_location() {
        let mut registry = ArchetypeRegistry::new();
        let mut locations = EntityLocationMap::default();
        let t1 = TypeId::of::<u32>();

        let first = Entity::test_only(5, 0);
        let second = Entity::test_only(6, 0);

        registry.set_entity_components(first, &[t1], &mut locations);
        registry.set_entity_components(second, &[t1], &mut locations);
        assert!(registry.remove_entity(first, &mut locations));
        assert!(locations.get(first).is_none());
        assert_eq!(
            locations.get(second),
            Some(EntityLocation {
                archetype_id: ArchetypeId::new(1),
                row: 0,
            }),
        );
    }
}
