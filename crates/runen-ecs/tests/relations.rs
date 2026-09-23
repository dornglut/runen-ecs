use runen_ecs::{
    BatchCommands, CommandError, Commands, Component, Directed, EntityError, LocalBatchCommands,
    LocalCommands, Relation, RelationError, SelfRelation, Symmetric, World,
};

#[derive(Component)]
struct Marker;

struct Owns;
impl Relation for Owns {
    type Kind = Directed;
}

struct Follows;
impl Relation for Follows {
    type Kind = Directed;
}

struct AlliedWith;
impl Relation for AlliedWith {
    type Kind = Symmetric;
}

struct DirectedSelf;
impl Relation for DirectedSelf {
    type Kind = Directed;
    const SELF: SelfRelation = SelfRelation::Allow;
}

struct SymmetricSelf;
impl Relation for SymmetricSelf {
    type Kind = Symmetric;
    const SELF: SelfRelation = SelfRelation::Allow;
}

#[test]
fn relation_types_are_independent_and_direction_is_type_level() {
    let mut world = World::new();
    let first = world.spawn(Marker).unwrap();
    let second = world.spawn(Marker).unwrap();

    assert!(world.relations_mut::<Owns>().insert(first, second).unwrap());
    assert!(world.relations_mut::<Follows>().insert(first, second).unwrap());
    assert_eq!(world.relations::<Owns>().len(), 1);
    assert_eq!(world.relations::<Follows>().len(), 1);

    assert!(world.relations::<Owns>().contains(first, second));
    assert!(!world.relations::<Owns>().contains(second, first));

    assert!(world
        .relations_mut::<AlliedWith>()
        .insert(first, second)
        .unwrap());
    assert!(world.relations::<AlliedWith>().contains(first, second));
    assert!(world.relations::<AlliedWith>().contains(second, first));
}

#[test]
fn duplicate_insert_and_remove_are_set_like() {
    let mut world = World::new();
    let first = world.spawn(Marker).unwrap();
    let second = world.spawn(Marker).unwrap();

    assert!(world.relations_mut::<Owns>().insert(first, second).unwrap());
    assert!(!world.relations_mut::<Owns>().insert(first, second).unwrap());
    assert_eq!(world.relations::<Owns>().len(), 1);

    assert!(world.relations_mut::<Owns>().remove(first, second).unwrap());
    assert!(!world.relations_mut::<Owns>().remove(first, second).unwrap());
    assert!(world.relations::<Owns>().is_empty());
}

#[test]
fn self_relation_policy_is_explicit_for_both_kinds() {
    let mut world = World::new();
    let entity = world.spawn(Marker).unwrap();

    assert!(matches!(
        world.relations_mut::<Owns>().insert(entity, entity),
        Err(RelationError::SelfReference { entity: found, .. }) if found == entity
    ));
    assert!(matches!(
        world
            .relations_mut::<AlliedWith>()
            .insert(entity, entity),
        Err(RelationError::SelfReference { entity: found, .. }) if found == entity
    ));

    assert!(world
        .relations_mut::<DirectedSelf>()
        .insert(entity, entity)
        .unwrap());
    assert!(world
        .relations_mut::<SymmetricSelf>()
        .insert(entity, entity)
        .unwrap());
    assert!(world.relations::<DirectedSelf>().contains(entity, entity));
    assert!(world.relations::<SymmetricSelf>().contains(entity, entity));
}

#[test]
fn mutation_validation_is_source_then_target_then_self_and_atomic() {
    let mut world = World::new();
    let source = world.spawn(Marker).unwrap();
    let target = world.spawn(Marker).unwrap();
    let stable = world.spawn(Marker).unwrap();
    world
        .relations_mut::<Owns>()
        .insert(source, target)
        .unwrap();

    let mut foreign_world = World::new();
    let foreign = foreign_world.spawn(Marker).unwrap();

    let error = world
        .relations_mut::<Owns>()
        .insert(foreign, foreign)
        .unwrap_err();
    assert!(matches!(
        error,
        RelationError::Entity(EntityError::ForeignWorld { entity }) if entity == foreign
    ));

    let freed = world.spawn(Marker).unwrap();
    world.despawn(freed).unwrap();
    let error = world
        .relations_mut::<Owns>()
        .insert(freed, foreign)
        .unwrap_err();
    assert!(matches!(
        error,
        RelationError::Entity(EntityError::AlreadyFreed { entity }) if entity == freed
    ));

    assert!(world.relations::<Owns>().contains(source, target));
    assert_eq!(world.relations::<Owns>().len(), 1);
    assert!(world.contains(stable));

    let self_error = world
        .relations_mut::<Owns>()
        .remove(source, source)
        .unwrap_err();
    assert!(matches!(
        self_error,
        RelationError::SelfReference { entity, .. } if entity == source
    ));
    assert!(world.relations::<Owns>().contains(source, target));
}

#[test]
fn non_failing_contains_treats_non_live_endpoints_as_absent() {
    let mut world = World::new();
    let first = world.spawn(Marker).unwrap();
    let second = world.spawn(Marker).unwrap();
    world.relations_mut::<Owns>().insert(first, second).unwrap();

    let mut foreign_world = World::new();
    let foreign = foreign_world.spawn(Marker).unwrap();
    assert!(!world.relations::<Owns>().contains(first, foreign));

    world.despawn(second).unwrap();
    assert!(!world.relations::<Owns>().contains(first, second));
}

#[test]
fn directed_views_are_deterministic_inverse_observations_of_one_edge_set() {
    let mut world = World::new();
    let a = world.spawn(Marker).unwrap();
    let b = world.spawn(Marker).unwrap();
    let c = world.spawn(Marker).unwrap();

    {
        let mut owns = world.relations_mut::<Owns>();
        owns.insert(b, c).unwrap();
        owns.insert(a, c).unwrap();
        owns.insert(a, b).unwrap();
    }

    assert_eq!(
        world.relations::<Owns>().iter().collect::<Vec<_>>(),
        vec![(a, b), (a, c), (b, c)]
    );
    assert_eq!(
        world
            .relations::<Owns>()
            .targets(a)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![b, c]
    );
    assert_eq!(
        world
            .relations::<Owns>()
            .sources(c)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![a, b]
    );
}

#[test]
fn symmetric_neighbors_and_iteration_are_canonical_and_deterministic() {
    let mut world = World::new();
    let a = world.spawn(Marker).unwrap();
    let b = world.spawn(Marker).unwrap();
    let c = world.spawn(Marker).unwrap();

    {
        let mut allied = world.relations_mut::<AlliedWith>();
        allied.insert(c, a).unwrap();
        allied.insert(b, a).unwrap();
        allied.insert(c, b).unwrap();
    }

    assert_eq!(
        world.relations::<AlliedWith>().iter().collect::<Vec<_>>(),
        vec![(a, b), (a, c), (b, c)]
    );
    assert_eq!(
        world
            .relations::<AlliedWith>()
            .neighbors(b)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![a, c]
    );
}

#[test]
fn isolated_live_entities_have_empty_adjacency_without_registration() {
    let mut world = World::new();
    let entity = world.spawn(Marker).unwrap();

    assert!(world
        .relations::<Owns>()
        .targets(entity)
        .unwrap()
        .next()
        .is_none());
    assert!(world
        .relations::<Owns>()
        .sources(entity)
        .unwrap()
        .next()
        .is_none());
    assert!(world
        .relations::<AlliedWith>()
        .neighbors(entity)
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn despawn_removes_incident_edges_and_generation_reuse_inherits_nothing() {
    let mut world = World::new();
    let old = world.spawn(Marker).unwrap();
    let other = world.spawn(Marker).unwrap();
    let third = world.spawn(Marker).unwrap();

    world.relations_mut::<Owns>().insert(old, other).unwrap();
    world.relations_mut::<Owns>().insert(third, old).unwrap();
    world
        .relations_mut::<AlliedWith>()
        .insert(old, other)
        .unwrap();

    world.despawn(old).unwrap();
    assert_eq!(world.relations::<Owns>().len(), 0);
    assert_eq!(world.relations::<AlliedWith>().len(), 0);

    let replacement = world.spawn(Marker).unwrap();
    assert_eq!(old.index(), replacement.index());
    assert_ne!(old.generation(), replacement.generation());
    assert!(!world.relations::<Owns>().contains(replacement, other));
    assert!(!world
        .relations::<AlliedWith>()
        .contains(replacement, other));
}

#[test]
fn rejected_despawn_does_not_change_relation_state() {
    let mut world = World::new();
    let first = world.spawn(Marker).unwrap();
    let second = world.spawn(Marker).unwrap();
    world.relations_mut::<Owns>().insert(first, second).unwrap();

    let mut foreign_world = World::new();
    let foreign = foreign_world.spawn(Marker).unwrap();
    assert!(matches!(
        world.despawn(foreign),
        Err(EntityError::ForeignWorld { .. })
    ));
    assert!(world.relations::<Owns>().contains(first, second));
}

#[test]
fn clear_entity_removes_only_incident_edges_of_one_relation_type() {
    let mut world = World::new();
    let a = world.spawn(Marker).unwrap();
    let b = world.spawn(Marker).unwrap();
    let c = world.spawn(Marker).unwrap();

    {
        let mut owns = world.relations_mut::<Owns>();
        owns.insert(a, b).unwrap();
        owns.insert(c, a).unwrap();
        owns.insert(b, c).unwrap();
    }
    world.relations_mut::<Follows>().insert(a, b).unwrap();

    assert_eq!(world.relations_mut::<Owns>().clear_entity(a).unwrap(), 2);
    assert_eq!(
        world.relations::<Owns>().iter().collect::<Vec<_>>(),
        vec![(b, c)]
    );
    assert!(world.relations::<Follows>().contains(a, b));
}

#[test]
fn deferred_relation_operations_share_direct_world_semantics() {
    let mut world = World::new();
    let a = world.spawn(Marker).unwrap();
    let b = world.spawn(Marker).unwrap();
    let c = world.spawn(Marker).unwrap();

    let mut commands = Commands::new();
    commands.insert_relation::<Owns>(a, b);
    commands.apply(&mut world).unwrap();
    assert!(world.relations::<Owns>().contains(a, b));

    let mut local = LocalCommands::new();
    local.remove_relation::<Owns>(a, b);
    local.insert_relation::<AlliedWith>(b, c);
    local.apply(&mut world).unwrap();
    assert!(!world.relations::<Owns>().contains(a, b));
    assert!(world.relations::<AlliedWith>().contains(b, c));

    let mut commands = Commands::new();
    commands.batch(|batch: &mut BatchCommands| {
        batch.insert_relation::<Owns>(a, b);
        batch.insert_relation::<Owns>(b, c);
        batch.clear_relations::<Owns>(b);
    });
    commands.apply(&mut world).unwrap();
    assert!(world.relations::<Owns>().is_empty());

    let mut local_batch = LocalBatchCommands::new();
    local_batch.insert_relation::<Owns>(a, c);
    local_batch.clear_relations::<AlliedWith>(b);
    local_batch.apply(&mut world).unwrap();
    assert!(world.relations::<Owns>().contains(a, c));
    assert!(world.relations::<AlliedWith>().is_empty());
}

#[test]
fn deferred_relation_failure_is_atomic_and_batch_stops_after_error() {
    let mut world = World::new();
    let a = world.spawn(Marker).unwrap();
    let b = world.spawn(Marker).unwrap();
    let c = world.spawn(Marker).unwrap();

    let mut foreign_world = World::new();
    let foreign = foreign_world.spawn(Marker).unwrap();

    let mut commands = Commands::new();
    commands.batch(|batch| {
        batch.insert_relation::<Owns>(a, b);
        batch.insert_relation::<Owns>(foreign, b);
        batch.insert_relation::<Owns>(a, c);
    });
    let error = commands.apply(&mut world).unwrap_err();
    assert!(matches!(
        error,
        CommandError::Relation(RelationError::Entity(EntityError::ForeignWorld { entity }))
            if entity == foreign
    ));
    assert!(world.relations::<Owns>().contains(a, b));
    assert!(!world.relations::<Owns>().contains(a, c));
}

#[test]
fn deferred_commands_validate_liveness_at_apply_time() {
    let mut world = World::new();
    let a = world.spawn(Marker).unwrap();
    let b = world.spawn(Marker).unwrap();

    let mut commands = Commands::new();
    commands.insert_relation::<Owns>(a, b);
    world.despawn(b).unwrap();

    assert!(matches!(
        commands.apply(&mut world),
        Err(CommandError::Relation(RelationError::Entity(
            EntityError::AlreadyFreed { entity }
        ))) if entity == b
    ));
    assert!(world.relations::<Owns>().is_empty());
}
