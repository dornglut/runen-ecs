// Owner: RunenECS World - Typed Relation Domain
//! Typed ECS relations over caller-owned Entity identities.
//!
//! RunenECS owns relation identity, liveness, lifecycle, and public access
//! semantics. RunenGraph is a private structural substrate for one authoritative
//! edge set per relation type.
use super::World;
use crate::entity::{Entity, EntityAllocator, EntityValidationSnapshot};
use crate::errors::{EntityError, RelationError};
use runen_graph::{Change, DirectedGraph, SelfRelationshipPolicy, SymmetricGraph};
use std::any::{TypeId, type_name};
use std::collections::{BTreeSet, HashMap};
use std::marker::PhantomData;
use std::ptr::NonNull;

mod sealed {
    pub trait Sealed {}
}

/// Framework-owned marker for directed relation semantics.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Directed;

/// Framework-owned marker for orientation-independent relation semantics.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Symmetric;

impl sealed::Sealed for Directed {}
impl sealed::Sealed for Symmetric {}

/// Sealed classification of a relation's directional semantics.
pub trait RelationKind: sealed::Sealed + 'static {}

impl RelationKind for Directed {}
impl RelationKind for Symmetric {}

/// Whether a relation type permits an entity to relate to itself.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SelfRelation {
    Allow,
    Forbid,
}

/// Maximum number of targets one source may hold for a directed relation.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SourceCardinality {
    Many,
    One,
}

/// Whether inserting an edge may create a directed cycle.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CyclePolicy {
    Allow,
    Forbid,
}

/// Generic constraints attached to one relation definition.
///
/// Fields stay private so later independently accepted constraints can extend
/// this value without making downstream relation definitions depend on layout.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct RelationConstraints {
    source_cardinality: SourceCardinality,
    cycle_policy: CyclePolicy,
}

impl RelationConstraints {
    pub const UNCONSTRAINED: Self = Self {
        source_cardinality: SourceCardinality::Many,
        cycle_policy: CyclePolicy::Allow,
    };

    pub const fn new() -> Self {
        Self::UNCONSTRAINED
    }

    pub const fn source_cardinality(mut self, value: SourceCardinality) -> Self {
        self.source_cardinality = value;
        self
    }

    pub const fn cycles(mut self, value: CyclePolicy) -> Self {
        self.cycle_policy = value;
        self
    }

    pub const fn source_cardinality_value(self) -> SourceCardinality {
        self.source_cardinality
    }

    pub const fn cycle_policy(self) -> CyclePolicy {
        self.cycle_policy
    }
}

impl Default for RelationConstraints {
    fn default() -> Self {
        Self::UNCONSTRAINED
    }
}

/// Defines one typed ECS relation domain.
///
/// The relation type is framework-internally identified by `TypeId`. `name`
/// is diagnostic text only and is not persistence, network, or replay identity.
pub trait Relation: 'static {
    type Kind: RelationKind;

    const SELF: SelfRelation = SelfRelation::Forbid;
    const CONSTRAINTS: RelationConstraints = RelationConstraints::UNCONSTRAINED;

    fn name() -> &'static str {
        type_name::<Self>()
    }
}

enum RelationGraph {
    Directed(DirectedGraph<Entity>),
    Symmetric(SymmetricGraph<Entity>),
}

pub(super) fn assert_supported_relation_definition<R: Relation>() {
    let constraints = R::CONSTRAINTS;
    let kind = TypeId::of::<R::Kind>();

    if kind == TypeId::of::<Directed>() {
        return;
    }
    if kind == TypeId::of::<Symmetric>() {
        assert!(
            constraints.source_cardinality_value() == SourceCardinality::Many
                && constraints.cycle_policy() == CyclePolicy::Allow,
            "relation {} declares constraints that are unsupported for symmetric relations",
            R::name()
        );
        return;
    }

    unreachable!("RelationKind is sealed to framework-owned kinds");
}

pub(super) struct RelationStore {
    graph: RelationGraph,
}

impl RelationStore {
    fn new<R: Relation>() -> Self {
        assert_supported_relation_definition::<R>();
        let policy = match R::SELF {
            SelfRelation::Allow => SelfRelationshipPolicy::Allow,
            SelfRelation::Forbid => SelfRelationshipPolicy::Forbid,
        };
        let kind = TypeId::of::<R::Kind>();
        let graph = if kind == TypeId::of::<Directed>() {
            RelationGraph::Directed(DirectedGraph::new(policy))
        } else if kind == TypeId::of::<Symmetric>() {
            RelationGraph::Symmetric(SymmetricGraph::new(policy))
        } else {
            unreachable!("RelationKind is sealed to framework-owned kinds")
        };
        Self { graph }
    }

    pub(super) fn assert_kind<R: Relation>(&self) {
        assert_supported_relation_definition::<R>();
        let kind = TypeId::of::<R::Kind>();
        let matches = if kind == TypeId::of::<Directed>() {
            matches!(&self.graph, RelationGraph::Directed(_))
        } else if kind == TypeId::of::<Symmetric>() {
            matches!(&self.graph, RelationGraph::Symmetric(_))
        } else {
            unreachable!("RelationKind is sealed to framework-owned kinds")
        };
        assert!(
            matches,
            "relation registry kind invariant violated for {}",
            R::name()
        );
    }

    fn directed(&self) -> Option<&DirectedGraph<Entity>> {
        match &self.graph {
            RelationGraph::Directed(graph) => Some(graph),
            RelationGraph::Symmetric(_) => None,
        }
    }

    fn symmetric(&self) -> Option<&SymmetricGraph<Entity>> {
        match &self.graph {
            RelationGraph::Directed(_) => None,
            RelationGraph::Symmetric(graph) => Some(graph),
        }
    }

    fn len(&self) -> usize {
        match &self.graph {
            RelationGraph::Directed(graph) => graph.relationship_count(),
            RelationGraph::Symmetric(graph) => graph.relationship_count(),
        }
    }

    fn contains(&self, first: Entity, second: Entity) -> bool {
        match &self.graph {
            RelationGraph::Directed(graph) => graph.contains_relationship(&first, &second),
            RelationGraph::Symmetric(graph) => graph.contains_relationship(&first, &second),
        }
    }

    fn insert<R: Relation>(
        &mut self,
        first: Entity,
        second: Entity,
    ) -> Result<bool, RelationError> {
        self.assert_kind::<R>();

        if self.contains(first, second) {
            return Ok(false);
        }

        if R::CONSTRAINTS.cycle_policy() == CyclePolicy::Forbid
            && self.directed_reaches(second, first)
        {
            return Err(RelationError::Cycle {
                relation: R::name(),
                source_entity: first,
                target_entity: second,
            });
        }

        let previous_target = if R::CONSTRAINTS.source_cardinality_value() == SourceCardinality::One
        {
            let graph = self
                .directed()
                .expect("source cardinality is only supported for directed relations");
            let mut outgoing = graph.outgoing(&first).into_iter().flatten().copied();
            let previous = outgoing.next();
            assert!(
                outgoing.next().is_none(),
                "source-one relation {} contains multiple targets for one source",
                R::name()
            );
            previous
        } else {
            None
        };

        if let Some(previous) = previous_target {
            assert!(
                self.remove(first, previous),
                "prevalidated source-one replacement lost its previous edge"
            );
        }

        let changed = self.insert_unconstrained(first, second);
        assert!(
            changed,
            "prevalidated relation insertion unexpectedly reported no change"
        );
        Ok(true)
    }

    fn insert_unconstrained(&mut self, first: Entity, second: Entity) -> bool {
        match &mut self.graph {
            RelationGraph::Directed(graph) => {
                let _ = graph.insert_node(first);
                let _ = graph.insert_node(second);
                match graph.insert_relationship(&first, &second) {
                    Ok(Change::Changed) => true,
                    Ok(Change::Unchanged) => false,
                    Err(error) => {
                        panic!("prevalidated directed relation mutation failed: {error}")
                    }
                }
            }
            RelationGraph::Symmetric(graph) => {
                let _ = graph.insert_node(first);
                let _ = graph.insert_node(second);
                match graph.insert_relationship(&first, &second) {
                    Ok(Change::Changed) => true,
                    Ok(Change::Unchanged) => false,
                    Err(error) => {
                        panic!("prevalidated symmetric relation mutation failed: {error}")
                    }
                }
            }
        }
    }

    fn directed_reaches(&self, start: Entity, goal: Entity) -> bool {
        let graph = self
            .directed()
            .expect("cycle constraints are only supported for directed relations");
        let mut pending = vec![start];
        let mut visited = BTreeSet::new();

        while let Some(entity) = pending.pop() {
            if entity == goal {
                return true;
            }
            if !visited.insert(entity) {
                continue;
            }
            if let Some(outgoing) = graph.outgoing(&entity) {
                pending.extend(outgoing.copied());
            }
        }

        false
    }

    fn remove(&mut self, first: Entity, second: Entity) -> bool {
        match &mut self.graph {
            RelationGraph::Directed(graph) => {
                if !graph.contains_node(&first) || !graph.contains_node(&second) {
                    return false;
                }
                match graph.remove_relationship(&first, &second) {
                    Ok(Change::Changed) => true,
                    Ok(Change::Unchanged) => false,
                    Err(error) => {
                        panic!("prevalidated directed relation mutation failed: {error}")
                    }
                }
            }
            RelationGraph::Symmetric(graph) => {
                if !graph.contains_node(&first) || !graph.contains_node(&second) {
                    return false;
                }
                match graph.remove_relationship(&first, &second) {
                    Ok(Change::Changed) => true,
                    Ok(Change::Unchanged) => false,
                    Err(error) => {
                        panic!("prevalidated symmetric relation mutation failed: {error}")
                    }
                }
            }
        }
    }

    pub(super) fn remove_entity(&mut self, entity: Entity) -> usize {
        match &mut self.graph {
            RelationGraph::Directed(graph) => graph.remove_node(&entity).relationships_removed,
            RelationGraph::Symmetric(graph) => graph.remove_node(&entity).relationships_removed,
        }
    }
}

#[derive(Copy, Clone)]
enum EntityValidationBacking<'world> {
    Serial {
        allocator: NonNull<EntityAllocator>,
        alive_entities: NonNull<BTreeSet<Entity>>,
        _marker: PhantomData<&'world ()>,
    },
    Worker {
        snapshot: NonNull<EntityValidationSnapshot>,
        alive_entities: NonNull<BTreeSet<Entity>>,
        _marker: PhantomData<&'world ()>,
    },
}

#[derive(Copy, Clone)]
pub(super) struct EntityValidationCapability<'world> {
    backing: EntityValidationBacking<'world>,
}

impl<'world> EntityValidationCapability<'world> {
    fn serial(
        allocator: &'world EntityAllocator,
        alive_entities: &'world BTreeSet<Entity>,
    ) -> Self {
        Self {
            backing: EntityValidationBacking::Serial {
                allocator: NonNull::from(allocator),
                alive_entities: NonNull::from(alive_entities),
                _marker: PhantomData,
            },
        }
    }

    pub(super) fn worker(
        snapshot: NonNull<EntityValidationSnapshot>,
        alive_entities: NonNull<BTreeSet<Entity>>,
    ) -> Self {
        Self {
            backing: EntityValidationBacking::Worker {
                snapshot,
                alive_entities,
                _marker: PhantomData,
            },
        }
    }

    fn validate(self, entity: Entity) -> Result<(), EntityError> {
        let (result, alive) = match self.backing {
            EntityValidationBacking::Serial {
                allocator,
                alive_entities,
                ..
            } => (unsafe { allocator.as_ref().validate(entity) }, unsafe {
                alive_entities.as_ref()
            }),
            EntityValidationBacking::Worker {
                snapshot,
                alive_entities,
                ..
            } => (unsafe { snapshot.as_ref().validate(entity) }, unsafe {
                alive_entities.as_ref()
            }),
        };
        result?;
        if alive.contains(&entity) {
            Ok(())
        } else {
            Err(EntityError::UnknownEntity { entity })
        }
    }

    fn contains(self, entity: Entity) -> bool {
        self.validate(entity).is_ok()
    }

    fn reborrow(&self) -> EntityValidationCapability<'_> {
        match self.backing {
            EntityValidationBacking::Serial {
                allocator,
                alive_entities,
                ..
            } => EntityValidationCapability {
                backing: EntityValidationBacking::Serial {
                    allocator,
                    alive_entities,
                    _marker: PhantomData,
                },
            },
            EntityValidationBacking::Worker {
                snapshot,
                alive_entities,
                ..
            } => EntityValidationCapability {
                backing: EntityValidationBacking::Worker {
                    snapshot,
                    alive_entities,
                    _marker: PhantomData,
                },
            },
        }
    }
}

pub(crate) struct RelationReadCapability<'world, R: Relation> {
    validation: EntityValidationCapability<'world>,
    store: Option<NonNull<RelationStore>>,
    _marker: PhantomData<&'world RelationStore>,
    _relation: PhantomData<fn() -> R>,
}

impl<R: Relation> Copy for RelationReadCapability<'_, R> {}

impl<R: Relation> Clone for RelationReadCapability<'_, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'world, R: Relation> RelationReadCapability<'world, R> {
    fn serial(world: &'world World) -> Self {
        assert_supported_relation_definition::<R>();
        let store = world.relation_store::<R>().map(NonNull::from);
        Self {
            validation: EntityValidationCapability::serial(&world.allocator, &world.alive_entities),
            store,
            _marker: PhantomData,
            _relation: PhantomData,
        }
    }

    pub(crate) unsafe fn from_world_ptr(world: NonNull<World>) -> Self {
        assert_supported_relation_definition::<R>();
        let world_ptr = world.as_ptr();
        let allocator = unsafe { &*std::ptr::addr_of!((*world_ptr).allocator) };
        let alive_entities = unsafe { &*std::ptr::addr_of!((*world_ptr).alive_entities) };
        let stores = unsafe { &*std::ptr::addr_of!((*world_ptr).relation_stores) };
        let store = stores
            .get(&TypeId::of::<R>())
            .map(Box::as_ref)
            .inspect(|store| store.assert_kind::<R>())
            .map(NonNull::from);
        Self {
            validation: EntityValidationCapability::serial(allocator, alive_entities),
            store,
            _marker: PhantomData,
            _relation: PhantomData,
        }
    }

    pub(super) fn worker(
        validation: EntityValidationCapability<'world>,
        store: Option<NonNull<RelationStore>>,
    ) -> Self {
        assert_supported_relation_definition::<R>();
        if let Some(store) = store {
            unsafe { store.as_ref().assert_kind::<R>() };
        }
        Self {
            validation,
            store,
            _marker: PhantomData,
            _relation: PhantomData,
        }
    }

    fn store(self) -> Option<&'world RelationStore> {
        self.store.map(|store| unsafe { store.as_ref() })
    }

    fn store_ptr(self) -> Option<NonNull<RelationStore>> {
        self.store
    }

    fn validate(self, entity: Entity) -> Result<(), EntityError> {
        self.validation.validate(entity)
    }

    fn contains_entity(self, entity: Entity) -> bool {
        self.validation.contains(entity)
    }
}

enum RelationWriteBacking<'world> {
    Serial {
        stores: NonNull<HashMap<TypeId, Box<RelationStore>>>,
        _marker: PhantomData<&'world mut HashMap<TypeId, Box<RelationStore>>>,
    },
    Worker {
        store: NonNull<RelationStore>,
        _marker: PhantomData<&'world mut RelationStore>,
    },
}

pub(crate) struct RelationWriteCapability<'world, R: Relation> {
    validation: EntityValidationCapability<'world>,
    backing: RelationWriteBacking<'world>,
    _relation: PhantomData<fn() -> R>,
}

impl<'world, R: Relation> RelationWriteCapability<'world, R> {
    fn serial(world: &'world mut World) -> Self {
        assert_supported_relation_definition::<R>();
        let validation =
            EntityValidationCapability::serial(&world.allocator, &world.alive_entities);
        let stores = NonNull::from(&mut world.relation_stores);
        Self {
            validation,
            backing: RelationWriteBacking::Serial {
                stores,
                _marker: PhantomData,
            },
            _relation: PhantomData,
        }
    }

    pub(crate) unsafe fn from_world_ptr(world: NonNull<World>) -> Self {
        assert_supported_relation_definition::<R>();
        let world_ptr = world.as_ptr();
        let allocator = unsafe { &*std::ptr::addr_of!((*world_ptr).allocator) };
        let alive_entities = unsafe { &*std::ptr::addr_of!((*world_ptr).alive_entities) };
        let stores =
            unsafe { NonNull::new_unchecked(std::ptr::addr_of_mut!((*world_ptr).relation_stores)) };
        Self {
            validation: EntityValidationCapability::serial(allocator, alive_entities),
            backing: RelationWriteBacking::Serial {
                stores,
                _marker: PhantomData,
            },
            _relation: PhantomData,
        }
    }

    pub(super) fn worker(
        validation: EntityValidationCapability<'world>,
        mut store: NonNull<RelationStore>,
    ) -> Self {
        assert_supported_relation_definition::<R>();
        unsafe { store.as_mut().assert_kind::<R>() };
        Self {
            validation,
            backing: RelationWriteBacking::Worker {
                store,
                _marker: PhantomData,
            },
            _relation: PhantomData,
        }
    }

    fn store(&self) -> Option<&RelationStore> {
        match &self.backing {
            RelationWriteBacking::Serial { stores, .. } => unsafe {
                stores
                    .as_ref()
                    .get(&TypeId::of::<R>())
                    .map(Box::as_ref)
                    .inspect(|store| store.assert_kind::<R>())
            },
            RelationWriteBacking::Worker { store, .. } => {
                let store = unsafe { store.as_ref() };
                store.assert_kind::<R>();
                Some(store)
            }
        }
    }

    fn store_mut(&mut self) -> Option<&mut RelationStore> {
        match &mut self.backing {
            RelationWriteBacking::Serial { stores, .. } => unsafe {
                stores
                    .as_mut()
                    .get_mut(&TypeId::of::<R>())
                    .map(Box::as_mut)
                    .inspect(|store| store.assert_kind::<R>())
            },
            RelationWriteBacking::Worker { store, .. } => {
                let store = unsafe { store.as_mut() };
                store.assert_kind::<R>();
                Some(store)
            }
        }
    }

    fn ensure_store(&mut self) -> &mut RelationStore {
        match &mut self.backing {
            RelationWriteBacking::Serial { stores, .. } => {
                let store = unsafe { stores.as_mut() }
                    .entry(TypeId::of::<R>())
                    .or_insert_with(|| Box::new(RelationStore::new::<R>()))
                    .as_mut();
                store.assert_kind::<R>();
                store
            }
            RelationWriteBacking::Worker { store, .. } => {
                let store = unsafe { store.as_mut() };
                store.assert_kind::<R>();
                store
            }
        }
    }

    fn read(&self) -> RelationReadCapability<'_, R> {
        let store = self.store().map(NonNull::from);
        RelationReadCapability {
            validation: self.validation.reborrow(),
            store,
            _marker: PhantomData,
            _relation: PhantomData,
        }
    }

    fn validate(&self, entity: Entity) -> Result<(), EntityError> {
        self.validation.validate(entity)
    }
}

impl World {
    /// Borrows one typed relation domain for read access.
    pub fn relations<R: Relation>(&self) -> Relations<'_, R> {
        Relations::from_capability(RelationReadCapability::serial(self))
    }

    /// Borrows one typed relation domain for mutation.
    pub fn relations_mut<R: Relation>(&mut self) -> RelationsMut<'_, R> {
        RelationsMut::from_capability(RelationWriteCapability::serial(self))
    }

    pub(super) fn relation_store<R: Relation>(&self) -> Option<&RelationStore> {
        self.relation_stores
            .get(&TypeId::of::<R>())
            .map(Box::as_ref)
            .inspect(|store| store.assert_kind::<R>())
    }

    pub(super) fn ensure_relation_store<R: Relation>(&mut self) -> &mut RelationStore {
        let store = self
            .relation_stores
            .entry(TypeId::of::<R>())
            .or_insert_with(|| Box::new(RelationStore::new::<R>()))
            .as_mut();
        store.assert_kind::<R>();
        store
    }

    pub(super) fn remove_entity_from_relations(&mut self, entity: Entity) {
        for store in self.relation_stores.values_mut() {
            let _ = store.remove_entity(entity);
        }
    }
}

/// Immutable access to one typed ECS relation domain.
pub struct Relations<'w, R: Relation> {
    capability: RelationReadCapability<'w, R>,
}

impl<'w, R: Relation> Relations<'w, R> {
    pub(crate) fn from_capability(capability: RelationReadCapability<'w, R>) -> Self {
        Self { capability }
    }

    /// Returns whether the relation contains the exact endpoint pair.
    ///
    /// Non-live endpoints are observed as absent.
    pub fn contains(&self, first: Entity, second: Entity) -> bool {
        relation_contains(self.capability, first, second)
    }

    pub fn len(&self) -> usize {
        self.capability.store().map_or(0, RelationStore::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterates relation edges in deterministic Entity ordering.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        relation_pairs(self.capability)
    }
}

impl<'w, R: Relation<Kind = Directed>> Relations<'w, R> {
    /// Returns an allocation-free view of outgoing targets in deterministic Entity ordering.
    pub fn targets(&self, source: Entity) -> Result<RelationEntities<'w>, RelationError> {
        self.capability.validate(source)?;
        Ok(RelationEntities::directed_targets(
            self.capability.store_ptr(),
            self.capability.validation,
            source,
        ))
    }

    /// Returns an allocation-free view of incoming sources in deterministic Entity ordering.
    pub fn sources(&self, target: Entity) -> Result<RelationEntities<'w>, RelationError> {
        self.capability.validate(target)?;
        Ok(RelationEntities::directed_sources(
            self.capability.store_ptr(),
            self.capability.validation,
            target,
        ))
    }
}

/// Exclusive access to one typed ECS relation domain.
pub struct RelationsMut<'w, R: Relation> {
    capability: RelationWriteCapability<'w, R>,
}

impl<'w, R: Relation> RelationsMut<'w, R> {
    pub(crate) fn from_capability(capability: RelationWriteCapability<'w, R>) -> Self {
        Self { capability }
    }

    pub fn contains(&self, first: Entity, second: Entity) -> bool {
        relation_contains(self.capability.read(), first, second)
    }

    pub fn len(&self) -> usize {
        self.capability.store().map_or(0, RelationStore::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        relation_pairs(self.capability.read())
    }

    /// Inserts one relation edge after ECS liveness and relation-policy validation.
    pub fn insert(&mut self, first: Entity, second: Entity) -> Result<bool, RelationError> {
        validate_pair::<R>(self.capability.validation, first, second)?;
        assert_supported_relation_definition::<R>();
        self.capability.ensure_store().insert::<R>(first, second)
    }

    /// Removes one relation edge after ECS liveness and relation-policy validation.
    pub fn remove(&mut self, first: Entity, second: Entity) -> Result<bool, RelationError> {
        validate_pair::<R>(self.capability.validation, first, second)?;
        assert_supported_relation_definition::<R>();
        let Some(store) = self.capability.store_mut() else {
            return Ok(false);
        };
        Ok(store.remove(first, second))
    }

    /// Removes every incident edge of this relation type for one live entity.
    pub fn clear_entity(&mut self, entity: Entity) -> Result<usize, RelationError> {
        self.capability.validate(entity)?;
        assert_supported_relation_definition::<R>();
        let Some(store) = self.capability.store_mut() else {
            return Ok(0);
        };
        Ok(store.remove_entity(entity))
    }
}

impl<R: Relation<Kind = Directed>> RelationsMut<'_, R> {
    pub fn targets(&self, source: Entity) -> Result<RelationEntities<'_>, RelationError> {
        let read = self.capability.read();
        read.validate(source)?;
        Ok(RelationEntities::directed_targets(
            read.store_ptr(),
            read.validation,
            source,
        ))
    }

    pub fn sources(&self, target: Entity) -> Result<RelationEntities<'_>, RelationError> {
        let read = self.capability.read();
        read.validate(target)?;
        Ok(RelationEntities::directed_sources(
            read.store_ptr(),
            read.validation,
            target,
        ))
    }
}

impl<'w, R: Relation<Kind = Symmetric>> Relations<'w, R> {
    /// Returns an allocation-free view of neighbors in deterministic Entity ordering.
    pub fn neighbors(&self, entity: Entity) -> Result<RelationEntities<'w>, RelationError> {
        self.capability.validate(entity)?;
        Ok(RelationEntities::symmetric_neighbors(
            self.capability.store_ptr(),
            self.capability.validation,
            entity,
        ))
    }
}

impl<R: Relation<Kind = Symmetric>> RelationsMut<'_, R> {
    pub fn neighbors(&self, entity: Entity) -> Result<RelationEntities<'_>, RelationError> {
        let read = self.capability.read();
        read.validate(entity)?;
        Ok(RelationEntities::symmetric_neighbors(
            read.store_ptr(),
            read.validation,
            entity,
        ))
    }
}

fn validate_pair<R: Relation>(
    validation: EntityValidationCapability<'_>,
    first: Entity,
    second: Entity,
) -> Result<(), RelationError> {
    validation.validate(first)?;
    validation.validate(second)?;
    if first == second && R::SELF == SelfRelation::Forbid {
        return Err(RelationError::SelfReference {
            relation: R::name(),
            entity: first,
        });
    }
    Ok(())
}

fn relation_contains<R: Relation>(
    capability: RelationReadCapability<'_, R>,
    first: Entity,
    second: Entity,
) -> bool {
    if !capability.contains_entity(first) || !capability.contains_entity(second) {
        return false;
    }
    capability
        .store()
        .is_some_and(|store| store.contains(first, second))
}

fn relation_pairs<R: Relation>(
    capability: RelationReadCapability<'_, R>,
) -> impl Iterator<Item = (Entity, Entity)> + '_ {
    let store = capability.store();
    let directed = store
        .and_then(RelationStore::directed)
        .into_iter()
        .flat_map(|graph| graph.relationships())
        .map(move |(first, second)| checked_pair(capability.validation, *first, *second));
    let symmetric = store
        .and_then(RelationStore::symmetric)
        .into_iter()
        .flat_map(|graph| graph.relationships())
        .map(move |(first, second)| checked_pair(capability.validation, *first, *second));
    directed.chain(symmetric)
}

/// Allocation-free read view over one relation adjacency.
pub struct RelationEntities<'w> {
    store: Option<NonNull<RelationStore>>,
    validation: EntityValidationCapability<'w>,
    endpoint: Entity,
    direction: RelationEntityDirection,
    _marker: PhantomData<&'w RelationStore>,
}

#[derive(Debug, Copy, Clone)]
enum RelationEntityDirection {
    DirectedTargets,
    DirectedSources,
    SymmetricNeighbors,
}

impl<'w> RelationEntities<'w> {
    fn directed_targets(
        store: Option<NonNull<RelationStore>>,
        validation: EntityValidationCapability<'w>,
        endpoint: Entity,
    ) -> Self {
        Self {
            store,
            validation,
            endpoint,
            direction: RelationEntityDirection::DirectedTargets,
            _marker: PhantomData,
        }
    }

    fn directed_sources(
        store: Option<NonNull<RelationStore>>,
        validation: EntityValidationCapability<'w>,
        endpoint: Entity,
    ) -> Self {
        Self {
            store,
            validation,
            endpoint,
            direction: RelationEntityDirection::DirectedSources,
            _marker: PhantomData,
        }
    }

    fn symmetric_neighbors(
        store: Option<NonNull<RelationStore>>,
        validation: EntityValidationCapability<'w>,
        endpoint: Entity,
    ) -> Self {
        Self {
            store,
            validation,
            endpoint,
            direction: RelationEntityDirection::SymmetricNeighbors,
            _marker: PhantomData,
        }
    }

    fn store(&self) -> Option<&RelationStore> {
        self.store.map(|store| unsafe { store.as_ref() })
    }

    /// Iterates the entities in deterministic Entity ordering.
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        let targets = self
            .store()
            .filter(|_| matches!(self.direction, RelationEntityDirection::DirectedTargets))
            .and_then(RelationStore::directed)
            .into_iter()
            .flat_map(move |graph| {
                graph
                    .outgoing(&self.endpoint)
                    .into_iter()
                    .flatten()
                    .copied()
            });

        let sources = self
            .store()
            .filter(|_| matches!(self.direction, RelationEntityDirection::DirectedSources))
            .and_then(RelationStore::directed)
            .into_iter()
            .flat_map(move |graph| {
                graph
                    .incoming(&self.endpoint)
                    .into_iter()
                    .flatten()
                    .copied()
            });

        let neighbors = self
            .store()
            .filter(|_| matches!(self.direction, RelationEntityDirection::SymmetricNeighbors))
            .and_then(RelationStore::symmetric)
            .into_iter()
            .flat_map(move |graph| {
                graph
                    .neighbors(&self.endpoint)
                    .into_iter()
                    .flatten()
                    .copied()
            });

        targets
            .chain(sources)
            .chain(neighbors)
            .map(move |entity| checked_entity(self.validation, entity))
    }

    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

fn checked_pair(
    validation: EntityValidationCapability<'_>,
    first: Entity,
    second: Entity,
) -> (Entity, Entity) {
    assert!(
        validation.contains(first) && validation.contains(second),
        "relation registry contains an edge to a non-live entity"
    );
    (first, second)
}

fn checked_entity(validation: EntityValidationCapability<'_>, entity: Entity) -> Entity {
    assert!(
        validation.contains(entity),
        "relation registry contains an edge to a non-live entity"
    );
    entity
}
