use runen_ecs::{BatchCommands, CommandError, Commands, EntityError, World};
use std::any::TypeId;

#[derive(Debug, PartialEq, Eq, runen_ecs::Component)]
struct A(u32);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component)]
struct B(u32);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component)]
struct C(u32);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component)]
struct D(u32);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component)]
struct NeverInsertedA;

#[derive(Debug, PartialEq, Eq, runen_ecs::Component)]
struct NeverInsertedB;

#[derive(runen_ecs::Bundle)]
struct EmptyDerivedBundle {}

#[derive(runen_ecs::Bundle)]
struct DerivedPair {
    a: A,
    b: B,
}

#[test]
fn empty_derived_bundle_spawns_and_preserves_empty_archetype() {
    let mut world = World::new();
    let entity = world
        .spawn(EmptyDerivedBundle {})
        .expect("empty spawn should succeed");

    assert!(world.contains(entity));
    assert_eq!(world.__entity_archetype_component_count(entity), Some(0));
}

#[test]
fn failed_multi_component_removal_preserves_all_components_and_observations() {
    let mut world = World::new();
    let entity = world.spawn(A(7)).expect("spawn should succeed");
    let changes_before = world.current_change_tick();
    let location_before = world
        .__entity_archetype_location(entity)
        .expect("entity should have an archetype location");

    let result = world.remove::<(A, B)>(entity);

    assert!(matches!(
        result,
        Err(EntityError::MissingComponent { component, .. })
            if component == std::any::type_name::<B>()
    ));
    assert_eq!(world.require::<A>(entity).expect("A must remain").0, 7);
    assert!(world.get::<B>(entity).is_none());
    assert_eq!(world.current_change_tick(), changes_before);
    assert_eq!(
        world.__entity_archetype_location(entity),
        Some(location_before)
    );
}

#[test]
fn duplicate_type_removal_keeps_existing_failure_semantics_without_partial_mutation() {
    let mut world = World::new();
    let entity = world.spawn(A(11)).expect("spawn should succeed");
    let changes_before = world.current_change_tick();

    let result = world.remove::<(A, A)>(entity);

    assert!(matches!(
        result,
        Err(EntityError::MissingComponent { component, .. })
            if component == std::any::type_name::<A>()
    ));
    assert_eq!(world.require::<A>(entity).expect("A must remain").0, 11);
    assert_eq!(world.current_change_tick(), changes_before);
}

#[test]
fn rejected_foreign_insert_preserves_colliding_local_structure_registration_and_index() {
    let mut first = World::new();
    let local = first.spawn(A(7)).expect("local spawn should succeed");
    assert!(first.ensure_component_index::<A, u32>(|value| value.0));
    assert_eq!(first.find_entity_by_index::<A, u32>(&7), Some(local));
    let local_location_before = first
        .__entity_archetype_location(local)
        .expect("local entity should have a location");
    let changes_before = first.current_change_tick();

    let mut second = World::new();
    let foreign = second.spawn(A(1)).expect("foreign spawn should succeed");
    assert_eq!(local.index(), foreign.index());
    assert_eq!(local.generation(), foreign.generation());

    let result = first.insert(foreign, (NeverInsertedA, NeverInsertedB));

    assert!(matches!(result, Err(EntityError::ForeignWorld { .. })));
    assert_eq!(first.require::<A>(local).expect("local A must remain").0, 7);
    assert_eq!(
        first.__entity_archetype_location(local),
        Some(local_location_before)
    );
    assert_eq!(first.find_entity_by_index::<A, u32>(&7), Some(local));
    assert_eq!(first.find_entity_by_index::<A, u32>(&1), None);
    assert!(!first.has_component_type(TypeId::of::<NeverInsertedA>()));
    assert!(!first.has_component_type(TypeId::of::<NeverInsertedB>()));
    assert_eq!(first.current_change_tick(), changes_before);
}

#[test]
fn freed_and_stale_insertions_do_not_register_or_mutate_replacement_state() {
    let mut world = World::new();
    let original = world.spawn(A(4)).expect("spawn should succeed");
    world.despawn(original).expect("despawn should succeed");
    let changes_after_despawn = world.current_change_tick();

    assert!(matches!(
        world.insert(original, NeverInsertedA),
        Err(EntityError::AlreadyFreed { .. })
    ));
    assert!(!world.has_component_type(TypeId::of::<NeverInsertedA>()));
    assert_eq!(world.current_change_tick(), changes_after_despawn);

    let replacement = world.spawn(A(9)).expect("slot reuse should succeed");
    assert_eq!(original.index(), replacement.index());
    assert_ne!(original.generation(), replacement.generation());
    let replacement_location_before = world
        .__entity_archetype_location(replacement)
        .expect("replacement should have a location");
    let changes_before_stale_insert = world.current_change_tick();

    assert!(matches!(
        world.insert(original, NeverInsertedB),
        Err(EntityError::StaleGeneration { .. })
    ));
    assert_eq!(
        world
            .require::<A>(replacement)
            .expect("replacement value must remain")
            .0,
        9
    );
    assert_eq!(
        world.__entity_archetype_location(replacement),
        Some(replacement_location_before)
    );
    assert!(!world.has_component_type(TypeId::of::<NeverInsertedB>()));
    assert_eq!(world.current_change_tick(), changes_before_stale_insert);
}

#[test]
fn supported_derived_and_tuple_bundles_commit_complete_structural_operations() {
    let mut world = World::new();
    let entity = world
        .spawn(DerivedPair { a: A(3), b: B(5) })
        .expect("derived bundle spawn should succeed");

    assert_eq!(world.require::<A>(entity).expect("A should exist").0, 3);
    assert_eq!(world.require::<B>(entity).expect("B should exist").0, 5);

    world
        .insert(entity, (C(8), D(13)))
        .expect("tuple insert should succeed");
    assert_eq!(world.require::<C>(entity).expect("C should exist").0, 8);
    assert_eq!(world.require::<D>(entity).expect("D should exist").0, 13);

    let removed = world
        .remove::<DerivedPair>(entity)
        .expect("derived bundle removal should succeed");
    assert_eq!(removed.a, A(3));
    assert_eq!(removed.b, B(5));
    assert!(world.get::<A>(entity).is_none());
    assert!(world.get::<B>(entity).is_none());
    assert_eq!(world.require::<C>(entity).expect("C should remain").0, 8);
    assert_eq!(world.require::<D>(entity).expect("D should remain").0, 13);
}

#[test]
fn component_observation_tick_matches_committed_storage_tick() {
    let mut world = World::new();
    let entity = world.spawn(A(1)).expect("spawn should succeed");
    let first_change_tick = world.current_change_tick();
    let (_, first_storage_tick) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A should have row metadata");
    assert_eq!(first_storage_tick, first_change_tick);

    world.insert(entity, A(2)).expect("update should succeed");
    let latest_change_tick = world.current_change_tick();
    let (_, latest_storage_tick) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A should keep row metadata");
    assert_eq!(latest_storage_tick, latest_change_tick);
    assert_eq!(world.require::<A>(entity).expect("A should exist").0, 2);
}

#[test]
fn commands_fail_stop_without_rolling_back_prior_success_or_partially_applying_failure() {
    let mut world = World::new();
    let entity = world.spawn(A(1)).expect("spawn should succeed");
    let mut commands = Commands::new();
    commands.insert(entity, C(2));
    commands.remove::<(A, B)>(entity);
    commands.insert(entity, D(3));

    let result = commands.apply(&mut world);

    assert!(matches!(
        result,
        Err(CommandError::Entity(EntityError::MissingComponent { .. }))
    ));
    assert_eq!(
        world
            .require::<C>(entity)
            .expect("earlier command commits")
            .0,
        2
    );
    assert_eq!(
        world
            .require::<A>(entity)
            .expect("failed removal is atomic")
            .0,
        1
    );
    assert!(world.get::<D>(entity).is_none());
}

#[test]
fn batch_commands_share_non_transactional_fail_stop_semantics() {
    let mut world = World::new();
    let entity = world.spawn(A(1)).expect("spawn should succeed");
    let mut batch = BatchCommands::new();
    batch.insert(entity, C(2));
    batch.remove::<(A, B)>(entity);
    batch.insert(entity, D(3));

    let result = batch.apply(&mut world);

    assert!(matches!(
        result,
        Err(CommandError::Entity(EntityError::MissingComponent { .. }))
    ));
    assert_eq!(
        world
            .require::<C>(entity)
            .expect("earlier command commits")
            .0,
        2
    );
    assert_eq!(
        world
            .require::<A>(entity)
            .expect("failed removal is atomic")
            .0,
        1
    );
    assert!(world.get::<D>(entity).is_none());
}
