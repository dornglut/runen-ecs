// Owner: RunenECS World - Typed Relation Domain
//! Typed ECS relations over caller-owned Entity identities.
//!
//! RunenECS owns relation identity, liveness, lifecycle, and public access
//! semantics. RunenGraph is a private structural substrate for one authoritative
//! edge set per relation type.
use super::World;
use crate::entity::Entity;
use crate::errors::RelationError;
use runen_graph::{Change, DirectedGraph, SelfRelationshipPolicy, SymmetricGraph};
use std::any::{TypeId, type_name};
use std::marker::PhantomData;

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

/// Defines one typed ECS relation domain.
///
/// The relation type is framework-internally identified by `TypeId`. `name`
/// is diagnostic text only and is not persistence, network, or replay identity.
pub trait Relation: 'static {
    type Kind: RelationKind;

    const SELF: SelfRelation = SelfRelation::Forbid;

    fn name() -> &'static str {
        type_name::<Self>()
    }
}

enum RelationGraph {
    Directed(DirectedGraph<Entity>),
    Symmetric(SymmetricGraph<Entity>),
}

pub(super) struct RelationStore {
    graph: RelationGraph,
}

impl RelationStore {
    fn new<R: Relation>() -> Self {
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

    fn assert_kind<R: Relation>(&self) {
        let kind = TypeId::of::<R::Kind>();
        let matches = if kind == TypeId::of::<Directed>() {
            matches!(self.graph, RelationGraph::Directed(_))
        } else if kind == TypeId::of::<Symmetric>() {
            matches!(self.graph, RelationGraph::Symmetric(_))
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

    fn insert(&mut self, first: Entity, second: Entity) -> bool {
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

impl World {
    /// Borrows one typed relation domain for read access.
    pub fn relations<R: Relation>(&self) -> Relations<'_, R> {
        Relations {
            world: self,
            _relation: PhantomData,
        }
    }

    /// Borrows one typed relation domain for mutation.
    pub fn relations_mut<R: Relation>(&mut self) -> RelationsMut<'_, R> {
        RelationsMut {
            world: self,
            _relation: PhantomData,
        }
    }

    fn relation_store<R: Relation>(&self) -> Option<&RelationStore> {
        self.relation_stores.get(&TypeId::of::<R>()).map(|store| {
            store.assert_kind::<R>();
            store
        })
    }

    fn ensure_relation_store<R: Relation>(&mut self) -> &mut RelationStore {
        let store = self
            .relation_stores
            .entry(TypeId::of::<R>())
            .or_insert_with(RelationStore::new::<R>);
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
    world: &'w World,
    _relation: PhantomData<fn() -> R>,
}

impl<R: Relation> Relations<'_, R> {
    /// Returns whether the relation contains the exact endpoint pair.
    ///
    /// Non-live endpoints are observed as absent.
    pub fn contains(&self, first: Entity, second: Entity) -> bool {
        relation_contains::<R>(self.world, first, second)
    }

    pub fn len(&self) -> usize {
        relation_len::<R>(self.world)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterates relation edges in deterministic Entity ordering.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        relation_pairs::<R>(self.world)
    }
}

impl<R: Relation<Kind = Directed>> Relations<'_, R> {
    /// Iterates outgoing targets in deterministic Entity ordering.
    pub fn targets(
        &self,
        source: Entity,
    ) -> Result<impl Iterator<Item = Entity> + '_, RelationError> {
        self.world.ensure_entity_exists(source)?;
        Ok(directed_targets::<R>(self.world, source))
    }

    /// Iterates incoming sources in deterministic Entity ordering.
    pub fn sources(
        &self,
        target: Entity,
    ) -> Result<impl Iterator<Item = Entity> + '_, RelationError> {
        self.world.ensure_entity_exists(target)?;
        Ok(directed_sources::<R>(self.world, target))
    }
}

impl<R: Relation<Kind = Symmetric>> Relations<'_, R> {
    /// Iterates orientation-independent neighbors in deterministic Entity ordering.
    pub fn neighbors(
        &self,
        entity: Entity,
    ) -> Result<impl Iterator<Item = Entity> + '_, RelationError> {
        self.world.ensure_entity_exists(entity)?;
        Ok(symmetric_neighbors::<R>(self.world, entity))
    }
}

/// Exclusive access to one typed ECS relation domain.
pub struct RelationsMut<'w, R: Relation> {
    world: &'w mut World,
    _relation: PhantomData<fn() -> R>,
}

impl<R: Relation> RelationsMut<'_, R> {
    pub fn contains(&self, first: Entity, second: Entity) -> bool {
        relation_contains::<R>(&*self.world, first, second)
    }

    pub fn len(&self) -> usize {
        relation_len::<R>(&*self.world)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        relation_pairs::<R>(&*self.world)
    }

    /// Inserts one relation edge after ECS liveness and relation-policy validation.
    pub fn insert(&mut self, first: Entity, second: Entity) -> Result<bool, RelationError> {
        validate_pair::<R>(&*self.world, first, second)?;
        Ok(self.world.ensure_relation_store::<R>().insert(first, second))
    }

    /// Removes one relation edge after ECS liveness and relation-policy validation.
    pub fn remove(&mut self, first: Entity, second: Entity) -> Result<bool, RelationError> {
        validate_pair::<R>(&*self.world, first, second)?;
        let Some(store) = self.world.relation_stores.get_mut(&TypeId::of::<R>()) else {
            return Ok(false);
        };
        store.assert_kind::<R>();
        Ok(store.remove(first, second))
    }

    /// Removes every incident edge of this relation type for one live entity.
    pub fn clear_entity(&mut self, entity: Entity) -> Result<usize, RelationError> {
        self.world.ensure_entity_exists(entity)?;
        let Some(store) = self.world.relation_stores.get_mut(&TypeId::of::<R>()) else {
            return Ok(0);
        };
        store.assert_kind::<R>();
        Ok(store.remove_entity(entity))
    }
}

impl<R: Relation<Kind = Directed>> RelationsMut<'_, R> {
    pub fn targets(
        &self,
        source: Entity,
    ) -> Result<impl Iterator<Item = Entity> + '_, RelationError> {
        self.world.ensure_entity_exists(source)?;
        Ok(directed_targets::<R>(&*self.world, source))
    }

    pub fn sources(
        &self,
        target: Entity,
    ) -> Result<impl Iterator<Item = Entity> + '_, RelationError> {
        self.world.ensure_entity_exists(target)?;
        Ok(directed_sources::<R>(&*self.world, target))
    }
}

impl<R: Relation<Kind = Symmetric>> RelationsMut<'_, R> {
    pub fn neighbors(
        &self,
        entity: Entity,
    ) -> Result<impl Iterator<Item = Entity> + '_, RelationError> {
        self.world.ensure_entity_exists(entity)?;
        Ok(symmetric_neighbors::<R>(&*self.world, entity))
    }
}

fn validate_pair<R: Relation>(
    world: &World,
    first: Entity,
    second: Entity,
) -> Result<(), RelationError> {
    world.ensure_entity_exists(first)?;
    world.ensure_entity_exists(second)?;
    if first == second && R::SELF == SelfRelation::Forbid {
        return Err(RelationError::SelfReference {
            relation: R::name(),
            entity: first,
        });
    }
    Ok(())
}

fn relation_contains<R: Relation>(world: &World, first: Entity, second: Entity) -> bool {
    if !world.contains(first) || !world.contains(second) {
        return false;
    }
    world
        .relation_store::<R>()
        .is_some_and(|store| store.contains(first, second))
}

fn relation_len<R: Relation>(world: &World) -> usize {
    world.relation_store::<R>().map_or(0, RelationStore::len)
}

fn relation_pairs<R: Relation>(world: &World) -> impl Iterator<Item = (Entity, Entity)> + '_ {
    let store = world.relation_store::<R>();
    let directed = store
        .and_then(RelationStore::directed)
        .into_iter()
        .flat_map(|graph| graph.relationships())
        .map(move |(first, second)| checked_pair(world, *first, *second));
    let symmetric = store
        .and_then(RelationStore::symmetric)
        .into_iter()
        .flat_map(|graph| graph.relationships())
        .map(move |(first, second)| checked_pair(world, *first, *second));
    directed.chain(symmetric)
}

fn directed_targets<R: Relation<Kind = Directed>>(
    world: &World,
    source: Entity,
) -> impl Iterator<Item = Entity> + '_ {
    world
        .relation_store::<R>()
        .and_then(RelationStore::directed)
        .into_iter()
        .flat_map(move |graph| {
            graph.relationships().filter_map(move |(candidate, target)| {
                (*candidate == source).then_some(*target)
            })
        })
        .map(move |target| checked_entity(world, target))
}

fn directed_sources<R: Relation<Kind = Directed>>(
    world: &World,
    target: Entity,
) -> impl Iterator<Item = Entity> + '_ {
    world
        .relation_store::<R>()
        .and_then(RelationStore::directed)
        .into_iter()
        .flat_map(move |graph| {
            graph.relationships().filter_map(move |(source, candidate)| {
                (*candidate == target).then_some(*source)
            })
        })
        .map(move |source| checked_entity(world, source))
}

fn symmetric_neighbors<R: Relation<Kind = Symmetric>>(
    world: &World,
    entity: Entity,
) -> impl Iterator<Item = Entity> + '_ {
    world
        .relation_store::<R>()
        .and_then(RelationStore::symmetric)
        .into_iter()
        .flat_map(move |graph| {
            graph.relationships().filter_map(move |(first, second)| {
                if *first == entity {
                    Some(*second)
                } else if *second == entity {
                    Some(*first)
                } else {
                    None
                }
            })
        })
        .map(move |neighbor| checked_entity(world, neighbor))
}

fn checked_pair(world: &World, first: Entity, second: Entity) -> (Entity, Entity) {
    assert!(
        world.contains(first) && world.contains(second),
        "relation registry contains an edge to a non-live entity"
    );
    (first, second)
}

fn checked_entity(world: &World, entity: Entity) -> Entity {
    assert!(
        world.contains(entity),
        "relation registry contains an edge to a non-live entity"
    );
    entity
}
