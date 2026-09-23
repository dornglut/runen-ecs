use super::change_tracking::panic_worker_projection_violation;
use super::mutation_journal::{ConcurrentMutationCapacity, MutationJournal};
use super::relation::{
    EntityValidationCapability, Relation, RelationReadCapability, RelationStore,
    RelationWriteCapability,
};
use super::{ChangeCursor, QueryCapability, ResourceCapability, World};
use crate::component::{Component, Resource};
use crate::entity::{Entity, EntityValidationSnapshot, WorldScopeId};
use crate::errors::ResourceError;
use crate::storage::ArchetypeExecutionBinding;
use std::any::{Any, TypeId, type_name};
use std::collections::{BTreeSet, HashMap};
use std::marker::PhantomData;
use std::ptr::NonNull;

type ErasedWorkerProjection = Box<dyn Any + Send>;
type WorkerComponentChangeMetadata = HashMap<Entity, (ChangeCursor, ChangeCursor)>;
type WorkerComponentMetadata = HashMap<TypeId, WorkerComponentChangeMetadata>;

struct SharedComponentProjection<T: Component> {
    values: HashMap<Entity, NonNull<T>>,
}

// Safety: this projection only produces shared `&T` values. Construction is
// restricted to `T: Sync`, and the structural lease prevents structural
// mutation or dense-column reallocation while these addresses are observed.
unsafe impl<T: Component + Sync> Send for SharedComponentProjection<T> {}

struct MutableComponentProjection<T: Component> {
    values: HashMap<Entity, NonNull<T>>,
}

// Safety: one prepared worker invocation owns this projection and construction
// is restricted to `T: Send`. Pairwise scheduler access validation prevents a
// second concurrent mutable projection of the same component domain.
unsafe impl<T: Component + Send> Send for MutableComponentProjection<T> {}

struct SharedResourceProjection<T: Resource> {
    value: NonNull<T>,
}

// Safety: this projection only produces shared `&T` values and is constructed
// only for `T: Sync`.
unsafe impl<T: Resource + Sync> Send for SharedResourceProjection<T> {}

struct MutableResourceProjection<T: Resource> {
    value: NonNull<T>,
}

// Safety: this projection is owned by one worker invocation and is constructed
// only for `T: Send` after scheduler conflict validation.
unsafe impl<T: Resource + Send> Send for MutableResourceProjection<T> {}

struct SharedRelationProjection {
    store: Option<NonNull<RelationStore>>,
}

// Safety: a shared projection only observes one boxed relation store while the
// structural lease prevents its removal. Scheduler access validation excludes
// a concurrent writer for the same relation type.
unsafe impl Send for SharedRelationProjection {}

struct MutableRelationProjection {
    store: NonNull<RelationStore>,
}

// Safety: one worker invocation owns this exclusive projection after scheduler
// validation. Relation stores contain only Entity structural identities, and
// boxed storage keeps the allocation stable across relation-registry rehashes.
unsafe impl Send for MutableRelationProjection {}

fn assert_relation_store_thread_traits() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RelationStore>();
}

/// Invoker-owned structural freeze for one controlled worker cohort.
///
/// The sole live `World` pointer never leaves this lease. Safe callers cannot
/// use the borrowed World while the lease exists; worker packages contain only
/// prepared payload projections and copied ECS metadata. Dense payloads may
/// relocate in later structural epochs, but not while this lease's workers can
/// observe their prepared pointers.
pub(crate) struct ParallelWorldLease<'world> {
    world: NonNull<World>,
    base_cursor: ChangeCursor,
    expected_cursor: ChangeCursor,
    capacity: ConcurrentMutationCapacity,
    _marker: PhantomData<&'world mut World>,
}

impl<'world> ParallelWorldLease<'world> {
    pub(crate) fn new(world: &'world mut World) -> Self {
        let base_cursor = world.current_change_cursor();
        Self {
            world: NonNull::from(world),
            base_cursor,
            expected_cursor: base_cursor,
            capacity: ConcurrentMutationCapacity::new(base_cursor),
            _marker: PhantomData,
        }
    }

    pub(crate) fn capacity(&self) -> ConcurrentMutationCapacity {
        self.capacity.clone()
    }

    pub(crate) fn builder(&self) -> WorkerWorldBuilder<'world> {
        // Safety: the lease owns the unique World borrow for `'world`. The
        // builder remains invoker-local and produces only narrow projections.
        WorkerWorldBuilder::new(self.world, self.base_cursor)
    }

    pub(crate) fn reconcile(&mut self, journal: MutationJournal) {
        assert_eq!(
            journal.concurrent_base_cursor(),
            Some(self.base_cursor),
            "worker journal must belong to the active structural-freeze cohort"
        );
        // Safety: worker threads have already joined before reconciliation and
        // the lease still owns the unique World authority. No structural
        // publication or dense-column relocation can occur before the lease is
        // dropped after reconciliation.
        let world = unsafe { self.world.as_mut() };
        assert_eq!(
            world.current_change_cursor(),
            self.expected_cursor,
            "World changed outside canonical worker-journal reconciliation"
        );
        journal.commit_concurrent(world);
        self.expected_cursor = world.current_change_cursor();
    }
}

/// Invoker-side type-directed builder for one worker invocation.
///
/// This type is deliberately not transferable. It may inspect the heterogeneous
/// World only while the invoker owns the structural-freeze lease, and it erases
/// a payload only after the concrete `Send`/`Sync` proof has been checked by the
/// calling parameter/query implementation.
pub(crate) struct WorkerWorldBuilder<'world> {
    world: NonNull<World>,
    world_scope: WorldScopeId,
    change_cursor: ChangeCursor,
    alive_entities: BTreeSet<Entity>,
    entity_validation: Option<EntityValidationSnapshot>,
    membership: HashMap<TypeId, BTreeSet<Entity>>,
    component_reads: HashMap<TypeId, ErasedWorkerProjection>,
    component_writes: HashMap<TypeId, ErasedWorkerProjection>,
    component_metadata: WorkerComponentMetadata,
    removed_records: HashMap<TypeId, Vec<(Entity, ChangeCursor)>>,
    resource_reads: HashMap<TypeId, ErasedWorkerProjection>,
    resource_writes: HashMap<TypeId, ErasedWorkerProjection>,
    relation_reads: HashMap<TypeId, SharedRelationProjection>,
    relation_writes: HashMap<TypeId, MutableRelationProjection>,
    _marker: PhantomData<&'world mut World>,
}

impl<'world> WorkerWorldBuilder<'world> {
    fn new(world: NonNull<World>, change_cursor: ChangeCursor) -> Self {
        // Safety: construction is restricted to `ParallelWorldLease::builder`.
        let world_ref = unsafe { world.as_ref() };
        Self {
            world,
            world_scope: world_ref.scope_id(),
            change_cursor,
            alive_entities: world_ref.alive_entities.clone(),
            entity_validation: None,
            membership: HashMap::new(),
            component_reads: HashMap::new(),
            component_writes: HashMap::new(),
            component_metadata: HashMap::new(),
            removed_records: HashMap::new(),
            resource_reads: HashMap::new(),
            resource_writes: HashMap::new(),
            relation_reads: HashMap::new(),
            relation_writes: HashMap::new(),
            _marker: PhantomData,
        }
    }

    pub(crate) fn prepare_membership(&mut self, type_id: TypeId) {
        if self.membership.contains_key(&type_id) {
            return;
        }
        let world = unsafe { self.world.as_ref() };
        let entities = self
            .alive_entities
            .iter()
            .copied()
            .filter(|entity| world.has_component_by_type_id(*entity, type_id))
            .collect();
        self.membership.insert(type_id, entities);
    }

    pub(crate) fn prepare_component_read<T: Component + Sync>(&mut self) {
        let type_id = TypeId::of::<T>();
        self.prepare_membership(type_id);
        if self.component_reads.contains_key(&type_id) {
            return;
        }
        assert!(
            !self.component_writes.contains_key(&type_id),
            "worker preparation mixed shared and exclusive component projections"
        );
        let world = unsafe { self.world.as_ref() };
        let mut values = HashMap::new();
        for entity in self.alive_entities.iter().copied() {
            if let Some(value) = world.archetype_component::<T>(entity) {
                values.insert(entity, NonNull::from(value));
            }
        }
        self.component_reads
            .insert(type_id, Box::new(SharedComponentProjection::<T> { values }));
    }

    pub(crate) fn prepare_component_write<T: Component + Send>(&mut self) {
        let type_id = TypeId::of::<T>();
        self.prepare_membership(type_id);
        if self.component_writes.contains_key(&type_id) {
            return;
        }
        assert!(
            !self.component_reads.contains_key(&type_id),
            "worker preparation mixed shared and exclusive component projections"
        );
        let entities = self.alive_entities.iter().copied().collect::<Vec<_>>();
        let world = unsafe { self.world.as_mut() };
        let mut values = HashMap::new();
        for entity in entities {
            if let Some(value) = world
                .archetype_registry
                .component_mut_ptr::<T>(entity, &world.entity_locations)
            {
                let value = NonNull::new(value).expect("component storage returned a null pointer");
                values.insert(entity, value);
            }
        }
        self.component_writes.insert(
            type_id,
            Box::new(MutableComponentProjection::<T> { values }),
        );
    }

    pub(crate) fn prepare_component_metadata<T: Component>(&mut self) {
        let type_id = TypeId::of::<T>();
        self.prepare_membership(type_id);
        if self.component_metadata.contains_key(&type_id) {
            return;
        }
        let world = unsafe { self.world.as_ref() };
        let mut metadata = HashMap::new();
        for entity in self.alive_entities.iter().copied() {
            if let Some(value) = world.archetype_component_metadata::<T>(entity) {
                metadata.insert(entity, value);
            }
        }
        self.component_metadata.insert(type_id, metadata);
    }

    pub(crate) fn prepare_removed<T: Component>(&mut self) {
        let type_id = TypeId::of::<T>();
        if self.removed_records.contains_key(&type_id) {
            return;
        }
        let world = unsafe { self.world.as_ref() };
        let records = world
            .removed_component_records
            .get(&type_id)
            .map(|records| {
                records
                    .iter()
                    .map(|record| (record.entity, record.tick))
                    .collect()
            })
            .unwrap_or_default();
        self.removed_records.insert(type_id, records);
    }

    pub(crate) fn prepare_resource_read<T: Resource + Sync>(
        &mut self,
    ) -> Result<(), ResourceError> {
        let type_id = TypeId::of::<T>();
        if self.resource_reads.contains_key(&type_id) {
            return Ok(());
        }
        assert!(
            !self.resource_writes.contains_key(&type_id),
            "worker preparation mixed shared and exclusive resource projections"
        );
        let world = unsafe { self.world.as_ref() };
        let value = world
            .resources
            .get(&type_id)
            .and_then(|resource| resource.downcast_ref::<T>())
            .ok_or(ResourceError::Missing {
                resource: type_name::<T>(),
            })?;
        self.resource_reads.insert(
            type_id,
            Box::new(SharedResourceProjection::<T> {
                value: NonNull::from(value),
            }),
        );
        Ok(())
    }

    pub(crate) fn prepare_resource_write<T: Resource + Send>(
        &mut self,
    ) -> Result<(), ResourceError> {
        let type_id = TypeId::of::<T>();
        if self.resource_writes.contains_key(&type_id) {
            return Ok(());
        }
        assert!(
            !self.resource_reads.contains_key(&type_id),
            "worker preparation mixed shared and exclusive resource projections"
        );
        let world = unsafe { self.world.as_mut() };
        let value = world
            .resources
            .get_mut(&type_id)
            .and_then(|resource| resource.downcast_mut::<T>())
            .ok_or(ResourceError::Missing {
                resource: type_name::<T>(),
            })?;
        self.resource_writes.insert(
            type_id,
            Box::new(MutableResourceProjection::<T> {
                value: NonNull::from(value),
            }),
        );
        Ok(())
    }

    fn prepare_relation_validation(&mut self) {
        if self.entity_validation.is_some() {
            return;
        }
        let world = unsafe { self.world.as_ref() };
        self.entity_validation = Some(world.allocator.validation_snapshot());
    }

    pub(crate) fn prepare_relation_read<R: Relation>(&mut self) {
        assert_relation_store_thread_traits();
        self.prepare_relation_validation();
        let type_id = TypeId::of::<R>();
        if self.relation_reads.contains_key(&type_id) {
            return;
        }
        assert!(
            !self.relation_writes.contains_key(&type_id),
            "worker preparation mixed shared and exclusive relation projections"
        );
        let world = unsafe { self.world.as_ref() };
        let store = world.relation_store::<R>().map(NonNull::from);
        self.relation_reads
            .insert(type_id, SharedRelationProjection { store });
    }

    pub(crate) fn prepare_relation_write<R: Relation>(&mut self) {
        assert_relation_store_thread_traits();
        self.prepare_relation_validation();
        let type_id = TypeId::of::<R>();
        if self.relation_writes.contains_key(&type_id) {
            return;
        }
        assert!(
            !self.relation_reads.contains_key(&type_id),
            "worker preparation mixed shared and exclusive relation projections"
        );
        let world = unsafe { self.world.as_mut() };
        let store = NonNull::from(world.ensure_relation_store::<R>());
        self.relation_writes
            .insert(type_id, MutableRelationProjection { store });
    }

    pub(crate) fn finish(self) -> PreparedWorkerWorld<'world> {
        PreparedWorkerWorld {
            world_scope: self.world_scope,
            change_cursor: self.change_cursor,
            alive_entities: self.alive_entities,
            entity_validation: self.entity_validation,
            membership: self.membership,
            component_reads: self.component_reads,
            component_writes: self.component_writes,
            component_metadata: self.component_metadata,
            removed_records: self.removed_records,
            resource_reads: self.resource_reads,
            resource_writes: self.resource_writes,
            relation_reads: self.relation_reads,
            relation_writes: self.relation_writes,
            _marker: PhantomData,
        }
    }
}

/// Fully prepared worker package. The whole heterogeneous World is no longer
/// reachable from this value.
pub(crate) struct PreparedWorkerWorld<'world> {
    world_scope: WorldScopeId,
    change_cursor: ChangeCursor,
    alive_entities: BTreeSet<Entity>,
    entity_validation: Option<EntityValidationSnapshot>,
    membership: HashMap<TypeId, BTreeSet<Entity>>,
    component_reads: HashMap<TypeId, ErasedWorkerProjection>,
    component_writes: HashMap<TypeId, ErasedWorkerProjection>,
    component_metadata: WorkerComponentMetadata,
    removed_records: HashMap<TypeId, Vec<(Entity, ChangeCursor)>>,
    resource_reads: HashMap<TypeId, ErasedWorkerProjection>,
    resource_writes: HashMap<TypeId, ErasedWorkerProjection>,
    relation_reads: HashMap<TypeId, SharedRelationProjection>,
    relation_writes: HashMap<TypeId, MutableRelationProjection>,
    _marker: PhantomData<&'world mut World>,
}

// Safety: every erased payload entry is itself `Send` because it was created by
// a type-directed constructor carrying the exact `T: Sync` shared-access or
// `T: Send` exclusive-access proof. Metadata is copied and contains no payload.
// The phantom lifetime ties this package to the invoker-owned structural lease;
// no pointer to World or any heterogeneous payload-owning container is stored.
unsafe impl Send for PreparedWorkerWorld<'_> {}

impl PreparedWorkerWorld<'_> {
    pub(crate) fn base_cursor(&self) -> ChangeCursor {
        self.change_cursor
    }

    pub(crate) fn authority(&mut self) -> WorkerWorldAuthority<'_> {
        WorkerWorldAuthority {
            query: WorkerQueryCapability {
                world_scope: self.world_scope,
                change_cursor: self.change_cursor,
                alive_entities: NonNull::from(&mut self.alive_entities),
                membership: NonNull::from(&mut self.membership),
                component_reads: NonNull::from(&mut self.component_reads),
                component_writes: NonNull::from(&mut self.component_writes),
                component_metadata: NonNull::from(&mut self.component_metadata),
                removed_records: NonNull::from(&mut self.removed_records),
                mutation_journal: None,
                _marker: PhantomData,
            },
            entity_validation: self.entity_validation.as_mut().map(NonNull::from),
            resource_reads: NonNull::from(&mut self.resource_reads),
            resource_writes: NonNull::from(&mut self.resource_writes),
            relation_reads: NonNull::from(&mut self.relation_reads),
            relation_writes: NonNull::from(&mut self.relation_writes),
            _marker: PhantomData,
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct WorkerWorldAuthority<'world> {
    query: WorkerQueryCapability<'world>,
    entity_validation: Option<NonNull<EntityValidationSnapshot>>,
    resource_reads: NonNull<HashMap<TypeId, ErasedWorkerProjection>>,
    resource_writes: NonNull<HashMap<TypeId, ErasedWorkerProjection>>,
    relation_reads: NonNull<HashMap<TypeId, SharedRelationProjection>>,
    relation_writes: NonNull<HashMap<TypeId, MutableRelationProjection>>,
    _marker: PhantomData<&'world mut ()>,
}

impl<'world> WorkerWorldAuthority<'world> {
    pub(crate) fn query_with_journal(
        self,
        journal: NonNull<MutationJournal>,
    ) -> QueryCapability<'world> {
        let mut query = self.query;
        query.mutation_journal = Some(journal);
        QueryCapability::from_worker(query)
    }

    pub(crate) fn resource<T: Resource>(
        self,
    ) -> Result<ResourceCapability<'world, T>, ResourceError> {
        let type_id = TypeId::of::<T>();
        let reads = unsafe { self.resource_reads.as_ref() };
        let projection = reads
            .get(&type_id)
            .and_then(|projection| projection.downcast_ref::<SharedResourceProjection<T>>())
            .ok_or(ResourceError::Missing {
                resource: type_name::<T>(),
            })?;
        Ok(ResourceCapability::worker_shared(projection.value))
    }

    pub(crate) fn resource_mut<T: Resource>(
        self,
        journal: NonNull<MutationJournal>,
    ) -> Result<ResourceCapability<'world, T>, ResourceError> {
        let type_id = TypeId::of::<T>();
        let writes = unsafe { self.resource_writes.as_ref() };
        let projection = writes
            .get(&type_id)
            .and_then(|projection| projection.downcast_ref::<MutableResourceProjection<T>>())
            .ok_or(ResourceError::Missing {
                resource: type_name::<T>(),
            })?;
        Ok(ResourceCapability::worker_mutable(
            projection.value,
            journal,
        ))
    }

    fn relation_validation(self) -> EntityValidationCapability<'world> {
        let snapshot = self.entity_validation.unwrap_or_else(|| {
            panic_worker_projection_violation(
                "worker relation access requested entity validation that was not prepared",
            )
        });
        EntityValidationCapability::worker(snapshot, self.query.alive_entities)
    }

    pub(crate) fn relation<R: Relation>(self) -> RelationReadCapability<'world, R> {
        let type_id = TypeId::of::<R>();
        let reads = unsafe { self.relation_reads.as_ref() };
        let projection = reads.get(&type_id).unwrap_or_else(|| {
            panic_worker_projection_violation(
                "worker relation read requested a relation type that was not prepared",
            )
        });
        RelationReadCapability::worker(self.relation_validation(), projection.store)
    }

    pub(crate) fn relation_mut<R: Relation>(self) -> RelationWriteCapability<'world, R> {
        let type_id = TypeId::of::<R>();
        let writes = unsafe { self.relation_writes.as_ref() };
        let projection = writes.get(&type_id).unwrap_or_else(|| {
            panic_worker_projection_violation(
                "worker relation write requested a relation type that was not prepared",
            )
        });
        RelationWriteCapability::worker(self.relation_validation(), projection.store)
    }
}

#[derive(Copy, Clone)]
pub(crate) struct WorkerQueryCapability<'world> {
    world_scope: WorldScopeId,
    change_cursor: ChangeCursor,
    alive_entities: NonNull<BTreeSet<Entity>>,
    membership: NonNull<HashMap<TypeId, BTreeSet<Entity>>>,
    component_reads: NonNull<HashMap<TypeId, ErasedWorkerProjection>>,
    component_writes: NonNull<HashMap<TypeId, ErasedWorkerProjection>>,
    component_metadata: NonNull<WorkerComponentMetadata>,
    removed_records: NonNull<HashMap<TypeId, Vec<(Entity, ChangeCursor)>>>,
    mutation_journal: Option<NonNull<MutationJournal>>,
    _marker: PhantomData<&'world mut ()>,
}

impl<'world> WorkerQueryCapability<'world> {
    pub(crate) fn current_change_tick(self) -> ChangeCursor {
        self.change_cursor
    }

    pub(crate) fn world_scope(self) -> WorldScopeId {
        self.world_scope
    }

    pub(crate) fn matching_entities_into(
        self,
        required_present: &[TypeId],
        excluded: &[TypeId],
        out: &mut Vec<Entity>,
    ) {
        let alive = unsafe { self.alive_entities.as_ref() };
        let membership = unsafe { self.membership.as_ref() };
        for type_id in required_present.iter().chain(excluded.iter()) {
            if !membership.contains_key(type_id) {
                panic_worker_projection_violation(
                    "worker query requested component membership that was not prepared",
                );
            }
        }
        out.clear();
        out.extend(alive.iter().copied().filter(|entity| {
            required_present.iter().all(|type_id| {
                membership
                    .get(type_id)
                    .expect("validated prepared membership must remain present")
                    .contains(entity)
            }) && excluded.iter().all(|type_id| {
                !membership
                    .get(type_id)
                    .expect("validated prepared membership must remain present")
                    .contains(entity)
            })
        }));
    }

    pub(crate) fn matching_archetype_bindings_into(
        self,
        _required_present: &[TypeId],
        _excluded: &[TypeId],
        out: &mut Vec<ArchetypeExecutionBinding>,
    ) -> bool {
        out.clear();
        false
    }

    pub(crate) fn archetype_entity_at(
        self,
        _archetype_index: usize,
        _row: usize,
    ) -> Option<Entity> {
        None
    }

    pub(crate) fn entity_matches_component_constraints(
        self,
        entity: Entity,
        required_present: &[TypeId],
        excluded: &[TypeId],
    ) -> bool {
        self.contains(entity)
            && required_present
                .iter()
                .all(|type_id| self.has_component_by_type_id(entity, *type_id))
            && excluded
                .iter()
                .all(|type_id| !self.has_component_by_type_id(entity, *type_id))
    }

    pub(crate) fn contains(self, entity: Entity) -> bool {
        unsafe { self.alive_entities.as_ref().contains(&entity) }
    }

    pub(crate) fn has_component_by_type_id(self, entity: Entity, type_id: TypeId) -> bool {
        let membership = unsafe { self.membership.as_ref() };
        let entities = membership.get(&type_id).unwrap_or_else(|| {
            panic_worker_projection_violation(
                "worker query requested component membership that was not prepared",
            )
        });
        entities.contains(&entity)
    }

    pub(crate) fn component<T: Component>(self, entity: Entity) -> Option<&'world T> {
        let type_id = TypeId::of::<T>();
        if !self.has_component_by_type_id(entity, type_id) {
            return None;
        }

        let reads = unsafe { self.component_reads.as_ref() };
        if let Some(projection) = reads
            .get(&type_id)
            .and_then(|projection| projection.downcast_ref::<SharedComponentProjection<T>>())
        {
            let value = projection.values.get(&entity).unwrap_or_else(|| {
                panic_worker_projection_violation(
                    "prepared shared component projection is missing a member payload",
                )
            });
            return Some(unsafe { &*value.as_ptr() });
        }

        let writes = unsafe { self.component_writes.as_ref() };
        if let Some(projection) = writes
            .get(&type_id)
            .and_then(|projection| projection.downcast_ref::<MutableComponentProjection<T>>())
        {
            let value = projection.values.get(&entity).unwrap_or_else(|| {
                panic_worker_projection_violation(
                    "prepared mutable component projection is missing a member payload",
                )
            });
            return Some(unsafe { &*value.as_ptr() });
        }

        panic_worker_projection_violation(
            "worker query requested a component payload that was not prepared",
        )
    }

    /// # Safety
    /// The worker-preparation proof and scheduler conflict validation must grant
    /// this invocation exclusive access to `T` for the full invocation.
    pub(crate) unsafe fn component_mut<T: Component>(
        mut self,
        entity: Entity,
    ) -> Option<&'world mut T> {
        let type_id = TypeId::of::<T>();
        if !self.has_component_by_type_id(entity, type_id) {
            return None;
        }
        let writes = unsafe { self.component_writes.as_mut() };
        let projection = writes
            .get_mut(&type_id)
            .and_then(|projection| projection.downcast_mut::<MutableComponentProjection<T>>())
            .unwrap_or_else(|| {
                panic_worker_projection_violation(
                    "worker query requested mutable component payload that was not prepared",
                )
            });
        let value = projection.values.get_mut(&entity).unwrap_or_else(|| {
            panic_worker_projection_violation(
                "prepared mutable component projection is missing a member payload",
            )
        });
        Some(unsafe { &mut *value.as_ptr() })
    }

    pub(crate) fn component_metadata<T: Component>(
        self,
        entity: Entity,
    ) -> Option<(ChangeCursor, ChangeCursor)> {
        let type_id = TypeId::of::<T>();
        if !self.has_component_by_type_id(entity, type_id) {
            return None;
        }
        let metadata = unsafe { self.component_metadata.as_ref() }
            .get(&type_id)
            .unwrap_or_else(|| {
                panic_worker_projection_violation(
                    "worker query requested component metadata that was not prepared",
                )
            });
        Some(*metadata.get(&entity).unwrap_or_else(|| {
            panic_worker_projection_violation("prepared component metadata is missing a member row")
        }))
    }

    pub(crate) fn mark_component_modified_by_id(self, entity: Entity, component_type: TypeId) {
        let mut journal = self.mutation_journal.unwrap_or_else(|| {
            panic_worker_projection_violation(
                "worker mutable query was created without a mutation journal",
            )
        });
        unsafe {
            journal
                .as_mut()
                .record_component_modified(entity, component_type)
        };
    }

    pub(crate) fn removed_component_records_current_window(
        self,
        component_type: TypeId,
        out: &mut Vec<(Entity, ChangeCursor)>,
    ) {
        out.clear();
        let records = unsafe { self.removed_records.as_ref() }
            .get(&component_type)
            .unwrap_or_else(|| {
                panic_worker_projection_violation(
                    "worker query requested removed-component records that were not prepared",
                )
            });
        out.extend(records.iter().copied());
    }
}
