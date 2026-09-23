//! Invocation-scoped projections owned by the `World` implementation.
//!
//! Serial capabilities project directly from the live World. Worker capabilities
//! are created only from the narrow prepared package built under the structural
//! freeze lease; they never carry a whole World or heterogeneous payload owner.

use super::World;
use super::change_tracking::{ChangeCursor, RemovedComponentRecord};
use super::component_indexes::{ComponentIndexKey, ComponentIndexStorage};
use super::mutation_journal::MutationJournal;
use super::parallel::WorkerQueryCapability;
use super::relation::{Relation, RelationReadCapability, RelationWriteCapability};
use crate::component::Component;
use crate::entity::{Entity, WorldScopeId};
use crate::errors::{ContiguousQueryError, ResourceError};
use crate::storage::{
    ArchetypeExecutionBinding, ArchetypeRegistry, ContiguousArchetypeSpan, EntityLocationMap,
};
use std::any::{TypeId, type_name};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::marker::PhantomData;
use std::ptr::NonNull;

/// The sole serial invocation-scoped authority from which narrow capabilities
/// are projected. It is never stored in a user-facing parameter value.
#[derive(Copy, Clone)]
pub(crate) struct WorldAuthority<'world> {
    world: NonNull<World>,
    _marker: PhantomData<&'world mut World>,
}

impl<'world> WorldAuthority<'world> {
    pub(crate) fn new(world: &'world mut World) -> Self {
        Self {
            world: NonNull::from(world),
            _marker: PhantomData,
        }
    }

    pub(crate) fn query_with_journal(
        self,
        journal: NonNull<MutationJournal>,
    ) -> QueryCapability<'world> {
        // Safety: the authority was constructed from the live invocation World;
        // the bridge immediately projects only owned query fields and the
        // invocation-owned journal remains alive for the same invocation.
        unsafe { QueryCapability::from_world_ptr_with_journal(self.world, journal) }
    }

    pub(crate) unsafe fn world_mut(mut self) -> &'world mut World {
        unsafe { self.world.as_mut() }
    }

    pub(crate) fn resource<T: crate::component::Resource>(
        self,
    ) -> Result<ResourceCapability<'world, T>, ResourceError> {
        // Safety: the authority lifetime is the invocation lifetime and the
        // resource bridge retains only the resource payload address; resource
        // structural mutation is excluded for the invocation lifetime.
        unsafe { World::resource_capability_from_ptr(self.world, false, None) }
    }

    pub(crate) fn resource_mut<T: crate::component::Resource>(
        self,
        journal: NonNull<MutationJournal>,
    ) -> Result<ResourceCapability<'world, T>, ResourceError> {
        // Safety: access validation rejects overlapping resource borrows before
        // this projection is manufactured.
        unsafe { World::resource_capability_from_ptr(self.world, true, Some(journal)) }
    }

    pub(crate) fn relation<R: Relation>(self) -> RelationReadCapability<'world, R> {
        // Safety: the authority lifetime is the current invocation and access
        // validation has already established this relation-type shared borrow.
        unsafe { RelationReadCapability::from_world_ptr(self.world) }
    }

    pub(crate) fn relation_mut<R: Relation>(self) -> RelationWriteCapability<'world, R> {
        // Safety: the authority lifetime is the current invocation and access
        // validation has already established this relation-type exclusive borrow.
        unsafe { RelationWriteCapability::from_world_ptr(self.world) }
    }
}

#[derive(Copy, Clone)]
struct SerialQueryCapability<'world> {
    world_scope: WorldScopeId,
    // Set only at the direct boundary that produced this capability; shared
    // serial queries must use immutable column bases, never `as_mut`.
    world_mutable: bool,
    alive_entities: NonNull<BTreeSet<Entity>>,
    archetype_registry: NonNull<ArchetypeRegistry>,
    entity_locations: NonNull<EntityLocationMap>,
    component_indexes: NonNull<RefCell<HashMap<ComponentIndexKey, Box<dyn ComponentIndexStorage>>>>,
    change_tick: NonNull<ChangeCursor>,
    component_change_ticks: NonNull<HashMap<TypeId, ChangeCursor>>,
    removed_component_records: NonNull<HashMap<TypeId, Vec<RemovedComponentRecord>>>,
    mutation_journal: Option<NonNull<MutationJournal>>,
    _marker: PhantomData<&'world World>,
}

#[derive(Copy, Clone)]
enum QueryCapabilityBacking<'world> {
    Serial(SerialQueryCapability<'world>),
    Worker(WorkerQueryCapability<'world>),
}

/// Narrow query-domain authority used by both serial and prepared-worker
/// extraction. The worker variant contains only prepared payload projections and
/// copied metadata.
#[doc(hidden)]
pub struct QueryCapability<'world> {
    backing: QueryCapabilityBacking<'world>,
}

impl<'world> Copy for QueryCapability<'world> {}
impl<'world> Clone for QueryCapability<'world> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'world> QueryCapability<'world> {
    pub(super) fn from_world(world: &'world World) -> Self {
        Self {
            backing: QueryCapabilityBacking::Serial(SerialQueryCapability {
                world_scope: world.scope_id(),
                world_mutable: false,
                alive_entities: NonNull::from(&world.alive_entities),
                archetype_registry: NonNull::from(&world.archetype_registry),
                entity_locations: NonNull::from(&world.entity_locations),
                component_indexes: NonNull::from(&world.component_indexes),
                change_tick: NonNull::from(&world.change_tick),
                component_change_ticks: NonNull::from(&world.component_change_ticks),
                removed_component_records: NonNull::from(&world.removed_component_records),
                mutation_journal: None,
                _marker: PhantomData,
            }),
        }
    }

    pub(super) fn from_world_mut(world: &'world mut World) -> Self {
        Self {
            backing: QueryCapabilityBacking::Serial(SerialQueryCapability {
                world_scope: world.scope_id(),
                world_mutable: true,
                alive_entities: NonNull::from(&mut world.alive_entities),
                archetype_registry: NonNull::from(&mut world.archetype_registry),
                entity_locations: NonNull::from(&mut world.entity_locations),
                component_indexes: NonNull::from(&mut world.component_indexes),
                change_tick: NonNull::from(&mut world.change_tick),
                component_change_ticks: NonNull::from(&mut world.component_change_ticks),
                removed_component_records: NonNull::from(&mut world.removed_component_records),
                mutation_journal: None,
                _marker: PhantomData,
            }),
        }
    }

    pub(super) unsafe fn from_world_ptr_with_journal(
        world: NonNull<World>,
        journal: NonNull<MutationJournal>,
    ) -> Self {
        unsafe { Self::from_world_ptr_with_journal_option(world, Some(journal)) }
    }

    unsafe fn from_world_ptr_with_journal_option(
        world: NonNull<World>,
        mutation_journal: Option<NonNull<MutationJournal>>,
    ) -> Self {
        let world_ptr = world.as_ptr();
        Self {
            backing: QueryCapabilityBacking::Serial(SerialQueryCapability {
                world_scope: unsafe { (*world_ptr).scope_id() },
                world_mutable: true,
                alive_entities: unsafe {
                    NonNull::new_unchecked(std::ptr::addr_of_mut!((*world_ptr).alive_entities))
                },
                archetype_registry: unsafe {
                    NonNull::new_unchecked(std::ptr::addr_of_mut!((*world_ptr).archetype_registry))
                },
                entity_locations: unsafe {
                    NonNull::new_unchecked(std::ptr::addr_of_mut!((*world_ptr).entity_locations))
                },
                component_indexes: unsafe {
                    NonNull::new_unchecked(std::ptr::addr_of_mut!((*world_ptr).component_indexes))
                },
                change_tick: unsafe {
                    NonNull::new_unchecked(std::ptr::addr_of_mut!((*world_ptr).change_tick))
                },
                component_change_ticks: unsafe {
                    NonNull::new_unchecked(std::ptr::addr_of_mut!(
                        (*world_ptr).component_change_ticks
                    ))
                },
                removed_component_records: unsafe {
                    NonNull::new_unchecked(std::ptr::addr_of_mut!(
                        (*world_ptr).removed_component_records
                    ))
                },
                mutation_journal,
                _marker: PhantomData,
            }),
        }
    }

    pub(crate) fn from_worker(worker: WorkerQueryCapability<'world>) -> Self {
        Self {
            backing: QueryCapabilityBacking::Worker(worker),
        }
    }

    pub(crate) fn current_change_tick(self) -> ChangeCursor {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => unsafe { *serial.change_tick.as_ptr() },
            QueryCapabilityBacking::Worker(worker) => worker.current_change_tick(),
        }
    }

    pub(crate) fn world_scope(self) -> WorldScopeId {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => serial.world_scope,
            QueryCapabilityBacking::Worker(worker) => worker.world_scope(),
        }
    }

    pub(crate) fn matching_entities_into(
        self,
        required_present: &[TypeId],
        excluded: &[TypeId],
        out: &mut Vec<Entity>,
    ) {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => unsafe {
                serial
                    .archetype_registry
                    .as_ref()
                    .collect_matching_entities(required_present, excluded, out);
            },
            QueryCapabilityBacking::Worker(worker) => {
                worker.matching_entities_into(required_present, excluded, out)
            }
        }
    }

    pub(crate) fn matching_archetype_bindings_into(
        self,
        required_present: &[TypeId],
        excluded: &[TypeId],
        out: &mut Vec<ArchetypeExecutionBinding>,
    ) -> bool {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => unsafe {
                serial
                    .archetype_registry
                    .as_ref()
                    .collect_matching_bindings(required_present, excluded, out)
            },
            QueryCapabilityBacking::Worker(worker) => {
                worker.matching_archetype_bindings_into(required_present, excluded, out)
            }
        }
    }

    pub(crate) fn collect_contiguous_spans(
        self,
        required_present: &[TypeId],
        excluded: &[TypeId],
        component_types: &[TypeId],
        mutable_types: &[TypeId],
    ) -> Result<Vec<ContiguousArchetypeSpan>, ContiguousQueryError> {
        // Worker projections contain borrowed row access, not Vec allocation
        // ownership; contiguous spans remain direct-serial only.
        match self.backing {
            QueryCapabilityBacking::Serial(mut serial) => {
                if serial.world_mutable {
                    unsafe {
                        serial.archetype_registry.as_mut().collect_contiguous_spans(
                            required_present,
                            excluded,
                            component_types,
                            mutable_types,
                        )
                    }
                    .map_err(|()| ContiguousQueryError::StorageInvariant)
                } else {
                    unsafe {
                        serial
                            .archetype_registry
                            .as_ref()
                            .collect_contiguous_spans_shared(
                                required_present,
                                excluded,
                                component_types,
                                mutable_types,
                            )
                    }
                    .map_err(|()| ContiguousQueryError::StorageInvariant)
                }
            }
            QueryCapabilityBacking::Worker(_) => Err(ContiguousQueryError::WorkerCapability),
        }
    }

    pub(crate) fn mark_contiguous_component_modified(
        self,
        entity: Entity,
        component_type: TypeId,
        changed_tick: NonNull<ChangeCursor>,
    ) {
        match self.backing {
            QueryCapabilityBacking::Serial(mut serial) if serial.world_mutable => {
                debug_assert!(serial.mutation_journal.is_none());
                let tick = Self::record_serial_component_change(
                    &mut serial,
                    entity,
                    component_type,
                    false,
                );
                // Safety: `changed_tick` addresses this component row's metadata
                // captured during segment preflight. The World borrow excludes
                // structural changes, and each (row, component) is recorded once.
                unsafe { changed_tick.as_ptr().write(tick) };
            }
            QueryCapabilityBacking::Serial(_) | QueryCapabilityBacking::Worker(_) => {
                unreachable!("mutable contiguous spans require a direct exclusive World borrow")
            }
        }
    }

    pub(crate) fn archetype_entity_at(self, archetype_index: usize, row: usize) -> Option<Entity> {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => unsafe {
                serial
                    .archetype_registry
                    .as_ref()
                    .entity_at(archetype_index, row)
            },
            QueryCapabilityBacking::Worker(worker) => {
                worker.archetype_entity_at(archetype_index, row)
            }
        }
    }

    pub(crate) fn entity_matches_component_constraints(
        self,
        entity: Entity,
        required_present: &[TypeId],
        excluded: &[TypeId],
    ) -> bool {
        match self.backing {
            QueryCapabilityBacking::Serial(_) => {
                self.contains(entity)
                    && required_present
                        .iter()
                        .all(|type_id| self.has_component_by_type_id(entity, *type_id))
                    && excluded
                        .iter()
                        .all(|type_id| !self.has_component_by_type_id(entity, *type_id))
            }
            QueryCapabilityBacking::Worker(worker) => {
                worker.entity_matches_component_constraints(entity, required_present, excluded)
            }
        }
    }

    pub(crate) fn contains(self, entity: Entity) -> bool {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => unsafe {
                serial.alive_entities.as_ref().contains(&entity)
            },
            QueryCapabilityBacking::Worker(worker) => worker.contains(entity),
        }
    }

    pub(crate) fn has_component_by_type_id(self, entity: Entity, type_id: TypeId) -> bool {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => {
                let locations = unsafe { serial.entity_locations.as_ref() };
                let Some(location) = locations.get(entity) else {
                    return false;
                };
                unsafe {
                    serial
                        .archetype_registry
                        .as_ref()
                        .component_types(location.archetype_id)
                }
                .is_some_and(|types| types.binary_search(&type_id).is_ok())
            }
            QueryCapabilityBacking::Worker(worker) => {
                worker.has_component_by_type_id(entity, type_id)
            }
        }
    }

    pub(crate) fn component<T: Component>(self, entity: Entity) -> Option<&'world T> {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => {
                let locations = unsafe { serial.entity_locations.as_ref() };
                let ptr = unsafe {
                    serial
                        .archetype_registry
                        .as_ref()
                        .component_ptr::<T>(entity, locations)
                }?;
                Some(unsafe { &*ptr })
            }
            QueryCapabilityBacking::Worker(worker) => worker.component::<T>(entity),
        }
    }

    /// # Safety
    /// The query access contract must contain the exclusive component borrow and
    /// structural mutation must remain frozen for `'world`.
    pub(crate) unsafe fn component_mut<T: Component>(
        self,
        entity: Entity,
    ) -> Option<&'world mut T> {
        match self.backing {
            QueryCapabilityBacking::Serial(mut serial) => {
                let locations = unsafe { serial.entity_locations.as_ref() };
                let registry = unsafe { serial.archetype_registry.as_mut() };
                let ptr = registry.component_mut_ptr::<T>(entity, locations)?;
                Some(unsafe { &mut *ptr })
            }
            QueryCapabilityBacking::Worker(worker) => unsafe { worker.component_mut::<T>(entity) },
        }
    }

    pub(crate) fn component_metadata<T: Component>(
        self,
        entity: Entity,
    ) -> Option<(ChangeCursor, ChangeCursor)> {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => {
                let locations = unsafe { serial.entity_locations.as_ref() };
                let metadata = unsafe {
                    serial
                        .archetype_registry
                        .as_ref()
                        .component_metadata::<T>(entity, locations)
                }?;
                Some((metadata.added_tick, metadata.changed_tick))
            }
            QueryCapabilityBacking::Worker(worker) => worker.component_metadata::<T>(entity),
        }
    }

    pub(crate) fn mark_component_modified_by_id(self, entity: Entity, component_type: TypeId) {
        match self.backing {
            QueryCapabilityBacking::Serial(mut serial) => {
                if let Some(mut journal) = serial.mutation_journal {
                    unsafe {
                        journal
                            .as_mut()
                            .record_component_modified(entity, component_type)
                    };
                    return;
                }
                let tick = Self::record_serial_component_change(
                    &mut serial,
                    entity,
                    component_type,
                    false,
                );
                let locations = unsafe { serial.entity_locations.as_ref() };
                let _ = unsafe {
                    serial
                        .archetype_registry
                        .as_mut()
                        .mark_component_changed_by_id(entity, component_type, tick, locations)
                };
            }
            QueryCapabilityBacking::Worker(worker) => {
                worker.mark_component_modified_by_id(entity, component_type)
            }
        }
    }

    pub(crate) fn mark_component_modified<T: Component>(self, entity: Entity) {
        if self.component::<T>(entity).is_some() {
            self.mark_component_modified_by_id(entity, TypeId::of::<T>());
        }
    }

    fn record_serial_component_change(
        serial: &mut SerialQueryCapability<'_>,
        entity: Entity,
        component_type: TypeId,
        removed: bool,
    ) -> ChangeCursor {
        let tick = unsafe {
            let tick = serial.change_tick.as_mut();
            *tick = super::change_tracking::advance_change_cursor(tick);
            *tick
        };
        unsafe {
            serial
                .component_change_ticks
                .as_mut()
                .insert(component_type, tick)
        };
        if removed {
            unsafe {
                serial
                    .removed_component_records
                    .as_mut()
                    .entry(component_type)
                    .or_default()
                    .push(RemovedComponentRecord { tick, entity })
            };
        }
        Self::mark_serial_component_indexes_dirty(*serial, component_type);
        tick
    }

    fn mark_serial_component_indexes_dirty(
        serial: SerialQueryCapability<'_>,
        component_type: TypeId,
    ) {
        let mut indexes = unsafe { serial.component_indexes.as_ref().borrow_mut() };
        for (index_key, index) in indexes.iter_mut() {
            if index_key.component_type == component_type {
                index.mark_dirty();
            }
        }
    }

    pub(crate) fn component_changed_for_entity_since<T: Component>(
        self,
        entity: Entity,
        tick: ChangeCursor,
    ) -> bool {
        self.component_metadata::<T>(entity)
            .is_some_and(|(_, changed_tick)| changed_tick > tick)
    }

    pub(crate) fn component_added_for_entity_since<T: Component>(
        self,
        entity: Entity,
        tick: ChangeCursor,
    ) -> bool {
        self.component_metadata::<T>(entity)
            .is_some_and(|(added_tick, _)| added_tick > tick)
    }

    pub(crate) fn removed_component_records_current_window(
        self,
        component_type: TypeId,
        out: &mut Vec<(Entity, ChangeCursor)>,
    ) {
        match self.backing {
            QueryCapabilityBacking::Serial(serial) => {
                out.clear();
                let records = unsafe { serial.removed_component_records.as_ref() };
                if let Some(records) = records.get(&component_type) {
                    out.extend(records.iter().map(|record| (record.entity, record.tick)));
                }
            }
            QueryCapabilityBacking::Worker(worker) => {
                worker.removed_component_records_current_window(component_type, out)
            }
        }
    }
}

/// Stable typed resource payload plus the separate narrow change recorder used
/// by `ResMut`.
#[doc(hidden)]
pub struct ResourceCapability<'world, T> {
    value: NonNull<T>,
    mutation: Option<ResourceMutationCapability<'world>>,
    _marker: PhantomData<&'world T>,
}

impl<'world, T> Copy for ResourceCapability<'world, T> {}
impl<'world, T> Clone for ResourceCapability<'world, T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Copy, Clone)]
enum ResourceMutationBacking<'world> {
    Serial {
        change_tick: NonNull<ChangeCursor>,
        resource_change_ticks: NonNull<HashMap<TypeId, ChangeCursor>>,
        mutation_journal: Option<NonNull<MutationJournal>>,
        _marker: PhantomData<&'world mut World>,
    },
    Worker {
        mutation_journal: NonNull<MutationJournal>,
        _marker: PhantomData<&'world mut ()>,
    },
}

#[derive(Copy, Clone)]
pub(crate) struct ResourceMutationCapability<'world> {
    backing: ResourceMutationBacking<'world>,
}

impl<'world> ResourceMutationCapability<'world> {
    pub(crate) fn mark_modified<T: 'static>(self) {
        let type_id = TypeId::of::<T>();
        match self.backing {
            ResourceMutationBacking::Serial {
                mut change_tick,
                mut resource_change_ticks,
                mutation_journal,
                ..
            } => {
                if let Some(mut journal) = mutation_journal {
                    unsafe { journal.as_mut().record_resource_modified(type_id) };
                    return;
                }
                let tick = unsafe {
                    let tick = change_tick.as_mut();
                    *tick = super::change_tracking::advance_change_cursor(tick);
                    *tick
                };
                unsafe { resource_change_ticks.as_mut().insert(type_id, tick) };
            }
            ResourceMutationBacking::Worker {
                mut mutation_journal,
                ..
            } => unsafe { mutation_journal.as_mut().record_resource_modified(type_id) },
        }
    }
}

impl World {
    /// Central World-owned bridge for ordinary query extraction.
    pub(crate) fn query_capability(&self) -> QueryCapability<'_> {
        QueryCapability::from_world(self)
    }

    pub(crate) fn query_capability_mut(&mut self) -> QueryCapability<'_> {
        QueryCapability::from_world_mut(self)
    }

    pub(crate) unsafe fn resource_capability_from_ptr<'world, T: crate::component::Resource>(
        world: NonNull<World>,
        mutable: bool,
        mutation_journal: Option<NonNull<MutationJournal>>,
    ) -> Result<ResourceCapability<'world, T>, ResourceError> {
        let world_ptr = world.as_ptr();
        let type_id = TypeId::of::<T>();
        if mutable {
            let value = unsafe {
                (*world_ptr)
                    .resources
                    .get_mut(&type_id)
                    .and_then(|resource| resource.downcast_mut::<T>())
            }
            .ok_or(ResourceError::Missing {
                resource: type_name::<T>(),
            })?;
            let mutation = ResourceMutationCapability {
                backing: ResourceMutationBacking::Serial {
                    change_tick: unsafe {
                        NonNull::new_unchecked(std::ptr::addr_of_mut!((*world_ptr).change_tick))
                    },
                    resource_change_ticks: unsafe {
                        NonNull::new_unchecked(std::ptr::addr_of_mut!(
                            (*world_ptr).resource_change_ticks
                        ))
                    },
                    mutation_journal,
                    _marker: PhantomData,
                },
            };
            Ok(ResourceCapability {
                value: NonNull::from(&mut *value),
                mutation: Some(mutation),
                _marker: PhantomData,
            })
        } else {
            let value = unsafe {
                (*world_ptr)
                    .resources
                    .get(&type_id)
                    .and_then(|resource| resource.downcast_ref::<T>())
            }
            .ok_or(ResourceError::Missing {
                resource: type_name::<T>(),
            })?;
            Ok(ResourceCapability {
                value: NonNull::from(value),
                mutation: None,
                _marker: PhantomData,
            })
        }
    }
}

impl<'world, T> ResourceCapability<'world, T> {
    pub(crate) fn worker_shared(value: NonNull<T>) -> Self {
        Self {
            value,
            mutation: None,
            _marker: PhantomData,
        }
    }

    pub(crate) fn worker_mutable(
        value: NonNull<T>,
        mutation_journal: NonNull<MutationJournal>,
    ) -> Self {
        Self {
            value,
            mutation: Some(ResourceMutationCapability {
                backing: ResourceMutationBacking::Worker {
                    mutation_journal,
                    _marker: PhantomData,
                },
            }),
            _marker: PhantomData,
        }
    }

    pub(crate) fn value(self) -> NonNull<T> {
        self.value
    }

    pub(crate) fn mutation(self) -> Option<ResourceMutationCapability<'world>> {
        self.mutation
    }
}
