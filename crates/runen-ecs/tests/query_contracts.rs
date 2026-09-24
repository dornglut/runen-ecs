use runen_ecs::prelude::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Component)]
struct Position;

fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<String>() {
        message
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        message
    } else {
        "<non-string panic>"
    }
}

#[test]
fn invalid_direct_query_reports_conflicting_component_borrow() {
    let world = World::new();

    let payload = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.query::<(&mut Position, &Position)>();
    }))
    .expect_err("overlapping mutable/shared direct query must panic during construction");

    let message = panic_message(payload.as_ref());
    assert!(message.contains("invalid query state"), "{message}");
    assert!(
        message.contains("conflicting component borrows"),
        "{message}"
    );
    assert!(message.contains("Position"), "{message}");
}

#[derive(Component)]
struct ReadA(u32);

#[derive(Component)]
struct ReadB(u32);

#[derive(Component)]
struct ReadC;

#[test]
fn read_only_required_queries_cover_matching_archetype_supersets() {
    let mut world = World::new();
    let first = world.spawn((ReadA(1), ReadB(10))).unwrap();
    let second = world.spawn((ReadA(2), ReadB(20), ReadC)).unwrap();
    world.spawn(ReadA(3)).unwrap();

    let pair = world.query::<(&ReadA, &ReadB)>();
    let mut pair_values = pair
        .iter(&world)
        .map(|(a, b)| (a.0, b.0))
        .collect::<Vec<_>>();
    pair_values.sort_unstable();
    assert_eq!(pair_values, vec![(1, 10), (2, 20)]);

    let duplicate_shared = world.query::<(&ReadA, &ReadA)>();
    let mut duplicate_values = duplicate_shared
        .iter(&world)
        .map(|(left, right)| (left.0, right.0))
        .collect::<Vec<_>>();
    duplicate_values.sort_unstable();
    assert_eq!(duplicate_values, vec![(1, 1), (2, 2), (3, 3)]);

    let entity_component = world.query::<(Entity, &ReadA)>();
    let mut entity_values = entity_component
        .iter(&world)
        .map(|(entity, a)| (a.0, entity))
        .collect::<Vec<_>>();
    entity_values.sort_unstable_by_key(|(value, _)| *value);
    assert_eq!(entity_values[0], (1, first));
    assert_eq!(entity_values[1], (2, second));
    assert_eq!(entity_values[2].0, 3);

    let triple = world.query::<(&ReadA, &ReadB, &ReadC)>();
    assert_eq!(
        triple
            .iter(&world)
            .map(|(a, b, _)| (a.0, b.0))
            .collect::<Vec<_>>(),
        vec![(2, 20)]
    );

    assert_eq!(world.query::<&ReadA>().iter(&world).count(), 3);

    let with_c = world.query::<(&ReadA, &ReadB)>().with::<ReadC>();
    assert_eq!(
        with_c
            .iter(&world)
            .map(|(a, b)| (a.0, b.0))
            .collect::<Vec<_>>(),
        vec![(2, 20)]
    );

    let without_c = world.query::<(&ReadA, &ReadB)>().without::<ReadC>();
    assert_eq!(
        without_c
            .iter(&world)
            .map(|(a, b)| (a.0, b.0))
            .collect::<Vec<_>>(),
        vec![(1, 10)]
    );
}

#[test]
fn read_only_query_state_observes_new_matching_archetypes_after_structure_changes() {
    let mut world = World::new();
    world.spawn((ReadA(1), ReadB(10))).unwrap();
    let query = world.query::<(&ReadA, &ReadB)>();

    assert_eq!(query.iter(&world).count(), 1);

    world.spawn((ReadA(2), ReadB(20), ReadC)).unwrap();

    let mut values = query
        .iter(&world)
        .map(|(a, b)| (a.0, b.0))
        .collect::<Vec<_>>();
    values.sort_unstable();
    assert_eq!(values, vec![(1, 10), (2, 20)]);
}
