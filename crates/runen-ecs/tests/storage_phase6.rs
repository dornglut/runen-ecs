use runen_ecs::prelude::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct A(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct B(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct C(i32);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct SeenCounts(Vec<usize>);

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

fn queue_spawn(mut commands: Commands) {
    commands.spawn(A(1));
}

fn record_a_count(mut query: Query<&A>, mut counts: ResMut<SeenCounts>) {
    counts.0.push(query.iter().count());
}

#[test]
fn archetype_location_tracking_updates_on_insert_remove_and_despawn() {
    let mut world = World::new();
    let first = world.spawn(A(1)).expect("spawn should succeed");
    let second = world.spawn(A(2)).expect("spawn should succeed");

    let first_before = world
        .__entity_archetype_location(first)
        .expect("first entity should be tracked");
    let second_before = world
        .__entity_archetype_location(second)
        .expect("second entity should be tracked");
    assert_eq!(world.__entity_archetype_component_count(first), Some(1));
    assert_eq!(world.__entity_archetype_component_count(second), Some(1));
    assert_eq!(first_before.0, second_before.0);
    assert_eq!(first_before.1, 0);
    assert_eq!(second_before.1, 1);

    world.insert(first, B(10)).unwrap();

    let first_after_insert = world
        .__entity_archetype_location(first)
        .expect("first entity should still be tracked");
    let second_after_insert = world
        .__entity_archetype_location(second)
        .expect("second entity should still be tracked");
    assert_eq!(world.__entity_archetype_component_count(first), Some(2));
    assert_eq!(world.__entity_archetype_component_count(second), Some(1));
    assert_ne!(first_after_insert.0, second_after_insert.0);
    assert_eq!(second_after_insert.1, 0);

    let _: B = world.remove(first).unwrap();
    let first_after_remove = world
        .__entity_archetype_location(first)
        .expect("first entity should still be tracked");
    assert_eq!(world.__entity_archetype_component_count(first), Some(1));
    assert_eq!(first_after_remove.0, second_after_insert.0);

    world.despawn(second).unwrap();
    assert!(world.__entity_archetype_location(second).is_none());
    assert_eq!(
        world
            .__entity_archetype_location(first)
            .expect("remaining entity should be tracked")
            .1,
        0
    );
}

#[test]
fn command_flush_boundary_remains_stage_based() {
    let mut world = World::new();
    world.insert_resource(SeenCounts(Vec::new()));
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (queue_spawn, record_a_count));

    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCounts>().unwrap().0, vec![0, 1, 2]);
}

#[test]
fn archetype_value_and_metadata_stay_aligned_across_migration() {
    let mut world = World::new();
    let first = world.spawn(A(11)).expect("spawn should succeed");
    let second = world.spawn(A(22)).expect("spawn should succeed");

    let (added_before, changed_before) = world
        .__entity_component_ticks::<A>(first)
        .expect("ticks should exist for inserted component");
    assert_eq!(added_before, changed_before);

    world.insert(first, B(5)).unwrap();
    assert_eq!(world.require::<A>(first).unwrap().0, 11);
    assert_eq!(world.require::<A>(second).unwrap().0, 22);
    let (added_after_insert, changed_after_insert) = world
        .__entity_component_ticks::<A>(first)
        .expect("ticks should remain aligned after migration");
    assert_eq!(added_after_insert, added_before);
    assert_eq!(changed_after_insert, changed_before);

    for value in world.query_state::<&mut A, ()>().iter(&mut world) {
        value.0 += 1;
    }
    let (added_after_mut, changed_after_mut) = world
        .__entity_component_ticks::<A>(first)
        .expect("ticks should exist after mutable query");
    assert_eq!(added_after_mut, added_before);
    assert!(changed_after_mut > changed_after_insert);

    let _: B = world.remove(first).unwrap();
    assert_eq!(world.require::<A>(first).unwrap().0, 12);
    let (added_after_remove, changed_after_remove) = world
        .__entity_component_ticks::<A>(first)
        .expect("ticks should stay aligned after remove migration");
    assert_eq!(added_after_remove, added_before);
    assert_eq!(changed_after_remove, changed_after_mut);
}

#[test]
fn despawn_clears_component_value_and_keeps_swapped_entity_access_valid() {
    let mut world = World::new();
    let first = world.spawn(A(1)).expect("spawn should succeed");
    let second = world.spawn(A(2)).expect("spawn should succeed");

    world.despawn(first).unwrap();
    assert!(world.get::<A>(first).is_none());
    assert_eq!(world.require::<A>(second).unwrap().0, 2);

    let remaining: Vec<_> = world
        .query_state::<(Entity, &A), ()>()
        .iter(&world)
        .map(|(entity, value)| (entity, value.0))
        .collect();
    assert_eq!(remaining, vec![(second, 2)]);
}

#[test]
fn repeated_swap_remove_repair_holds_across_chained_migrations() {
    let mut world = World::new();
    let first = world.spawn(A(1)).expect("spawn should succeed");
    let second = world.spawn(A(2)).expect("spawn should succeed");
    let third = world.spawn(A(3)).expect("spawn should succeed");

    assert_eq!(
        world
            .__entity_archetype_location(third)
            .expect("third should be tracked")
            .1,
        2
    );

    world.insert(first, B(10)).unwrap();
    assert_eq!(
        world
            .__entity_archetype_location(third)
            .expect("third should remain tracked")
            .1,
        0
    );
    assert_eq!(
        world
            .__entity_archetype_location(second)
            .expect("second should remain tracked")
            .1,
        1
    );
    assert_eq!(world.require::<A>(third).unwrap().0, 3);

    world.insert(second, B(20)).unwrap();
    assert_eq!(
        world
            .__entity_archetype_location(third)
            .expect("third should remain tracked")
            .1,
        0
    );

    let _: B = world.remove(first).unwrap();
    assert_eq!(
        world
            .__entity_archetype_location(third)
            .expect("third should remain tracked")
            .1,
        0
    );
    assert_eq!(
        world
            .__entity_archetype_location(first)
            .expect("first should move back into A archetype")
            .1,
        1
    );

    let _: B = world.remove(second).unwrap();
    let first_location = world
        .__entity_archetype_location(first)
        .expect("first should be tracked");
    let second_location = world
        .__entity_archetype_location(second)
        .expect("second should be tracked");
    let third_location = world
        .__entity_archetype_location(third)
        .expect("third should be tracked");
    assert_eq!(first_location.0, second_location.0);
    assert_eq!(second_location.0, third_location.0);
    assert_eq!(third_location.1, 0);
    assert_eq!(first_location.1, 1);
    assert_eq!(second_location.1, 2);
    assert_eq!(world.require::<A>(first).unwrap().0, 1);
    assert_eq!(world.require::<A>(second).unwrap().0, 2);
    assert_eq!(world.require::<A>(third).unwrap().0, 3);
}

#[test]
fn row_metadata_stays_consistent_across_multi_component_remove_reinsert_cycles() {
    let mut world = World::new();
    let entity = world.spawn(A(10)).expect("spawn should succeed");

    let (a_added_initial, mut a_changed_last) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist after spawn");
    let mut previous_b_added = None;

    for cycle in 0..3 {
        world
            .insert(entity, (B(100 + cycle), C(200 + cycle)))
            .unwrap();
        let (b_added, b_changed) = world
            .__entity_component_ticks::<B>(entity)
            .expect("B ticks should exist after insert");
        assert_eq!(b_added, b_changed);
        if let Some(previous) = previous_b_added {
            assert!(b_added > previous);
        }
        previous_b_added = Some(b_added);

        for (a, b) in world.query_state::<(&mut A, &mut B), ()>().iter(&mut world) {
            a.0 += 1;
            b.0 += 5;
        }

        let (a_added_after_mut, a_changed_after_mut) = world
            .__entity_component_ticks::<A>(entity)
            .expect("A ticks should exist after mutation");
        let (b_added_after_mut, b_changed_after_mut) = world
            .__entity_component_ticks::<B>(entity)
            .expect("B ticks should exist after mutation");
        assert_eq!(a_added_after_mut, a_added_initial);
        assert!(a_changed_after_mut > a_changed_last);
        assert_eq!(b_added_after_mut, b_added);
        assert!(b_changed_after_mut > b_changed);
        a_changed_last = a_changed_after_mut;

        let _: C = world.remove(entity).unwrap();
        let _: B = world.remove(entity).unwrap();
        assert!(world.get::<B>(entity).is_none());
        assert!(world.get::<C>(entity).is_none());

        let (a_added_after_remove, a_changed_after_remove) = world
            .__entity_component_ticks::<A>(entity)
            .expect("A ticks should remain after remove/reinsert cycle");
        assert_eq!(a_added_after_remove, a_added_initial);
        assert_eq!(a_changed_after_remove, a_changed_last);
    }
}

#[test]
fn despawn_after_multiple_migrations_preserves_swapped_entity_consistency() {
    let mut world = World::new();
    let anchor = world.spawn(A(1)).expect("spawn should succeed");
    let migrating = world.spawn(A(2)).expect("spawn should succeed");

    world.insert(migrating, B(20)).unwrap();
    world.insert(migrating, C(200)).unwrap();
    let _: B = world.remove(migrating).unwrap();
    world.insert(migrating, B(25)).unwrap();
    let partner = world
        .spawn((A(3), B(30), C(300)))
        .expect("spawn should succeed");

    let partner_before = world
        .__entity_archetype_location(partner)
        .expect("partner should be tracked before despawn");
    let migrating_before = world
        .__entity_archetype_location(migrating)
        .expect("migrating should be tracked before despawn");
    assert_eq!(partner_before.0, migrating_before.0);

    world.despawn(migrating).unwrap();
    assert!(!world.contains(migrating));
    assert!(world.__entity_archetype_location(migrating).is_none());
    assert!(world.get::<A>(migrating).is_none());
    assert!(world.get::<B>(migrating).is_none());
    assert!(world.get::<C>(migrating).is_none());

    assert_eq!(world.require::<A>(anchor).unwrap().0, 1);
    assert_eq!(world.require::<A>(partner).unwrap().0, 3);
    assert_eq!(world.require::<B>(partner).unwrap().0, 30);
    assert_eq!(world.require::<C>(partner).unwrap().0, 300);

    let partner_after = world
        .__entity_archetype_location(partner)
        .expect("partner should remain tracked after swap-remove despawn");
    assert_eq!(partner_after.0, partner_before.0);
    assert_eq!(partner_after.1, migrating_before.1);
}
