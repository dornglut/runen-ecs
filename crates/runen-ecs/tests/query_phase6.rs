use runen_ecs::prelude::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct A(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct B(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct C(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Disabled;

#[derive(Copy, Clone)]
struct QueryUpdate;

impl ScheduleLabel for QueryUpdate {
    fn name() -> &'static str {
        "QueryUpdate"
    }
}

#[derive(Copy, Clone)]
struct QueueSet;

impl SystemSet for QueueSet {
    fn name() -> &'static str {
        "QueueSet"
    }
}

#[derive(Copy, Clone)]
struct ObserveSet;

impl SystemSet for ObserveSet {
    fn name() -> &'static str {
        "ObserveSet"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct QuerySpawnGate(bool);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct QueryAddedCounts(Vec<usize>);

#[test]
fn dominant_query_forms_remain_correct_with_archetype_matching() {
    let mut world = World::new();
    let e1 = world.spawn((A(1), B(10))).expect("spawn should succeed");
    let e2 = world.spawn(A(2)).expect("spawn should succeed");
    let e3 = world.spawn((A(3), B(30))).expect("spawn should succeed");

    for a in world.query_state::<&mut A, ()>().iter(&mut world) {
        a.0 += 1;
    }

    for (a, b) in world.query_state::<(&mut A, &B), ()>().iter(&mut world) {
        a.0 += b.0;
    }

    for (a, b) in world.query_state::<(&mut A, &mut B), ()>().iter(&mut world) {
        a.0 += b.0;
        b.0 += 1;
    }

    assert_eq!(world.require::<A>(e1).unwrap().0, 22);
    assert_eq!(world.require::<B>(e1).unwrap().0, 11);
    assert_eq!(world.require::<A>(e2).unwrap().0, 3);
    assert_eq!(world.require::<A>(e3).unwrap().0, 64);
    assert_eq!(world.require::<B>(e3).unwrap().0, 31);
}

#[test]
fn changed_added_and_remove_reinsert_semantics_are_preserved() {
    let mut world = World::new();
    let entity = world.spawn(A(5)).expect("spawn should succeed");

    let added = world.query_state::<(Entity, &A), Added<A>>();
    assert_eq!(
        added
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(added.iter(&world).next().is_none());

    let changed = world.query_state::<(Entity, &A), Changed<A>>();
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed.iter(&world).next().is_none());

    let _: A = world.remove(entity).unwrap();
    assert!(added.iter(&world).next().is_none());
    assert!(changed.iter(&world).next().is_none());

    world.insert(entity, A(9)).unwrap();
    assert_eq!(
        added
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
}

#[test]
fn reused_query_state_observes_new_matching_archetypes_and_fallback_forms() {
    let mut world = World::new();
    let evolving = world.spawn(A(7)).expect("spawn should succeed");
    let hidden = world.spawn((A(8), Disabled)).expect("spawn should succeed");

    let dominant = world.query_state::<(&mut A, &B), ()>();
    assert!(dominant.iter(&mut world).next().is_none());
    world.insert(evolving, B(2)).unwrap();

    let mut seen = Vec::new();
    for (a, b) in dominant.iter(&mut world) {
        a.0 += b.0;
        seen.push(a.0);
    }
    assert_eq!(seen, vec![9]);

    let fallback = world
        .query_state::<(Entity, &A), ()>()
        .without::<Disabled>();
    let visible: Vec<_> = fallback
        .iter(&world)
        .map(|(entity, a)| (entity, a.0))
        .collect();
    assert_eq!(visible, vec![(evolving, 9)]);
    assert!(visible.iter().all(|(entity, _)| *entity != hidden));
}

#[test]
fn single_mut_query_preserves_changed_tracking() {
    let mut world = World::new();
    let first = world.spawn(A(1)).expect("spawn should succeed");
    let second = world.spawn(A(2)).expect("spawn should succeed");

    let changed = world.query_state::<(Entity, &A), Changed<A>>();
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![first, second]
    );
    assert!(changed.iter(&world).next().is_none());

    for value in world.query_state::<&mut A, ()>().iter(&mut world) {
        value.0 += 10;
    }

    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![first, second]
    );
    assert!(changed.iter(&world).next().is_none());
}

#[test]
fn single_mut_query_with_without_filter_keeps_fallback_semantics() {
    let mut world = World::new();
    let active = world.spawn(A(5)).expect("spawn should succeed");
    let hidden = world.spawn((A(6), Disabled)).expect("spawn should succeed");

    let query = world
        .query_state::<(Entity, &mut A), ()>()
        .without::<Disabled>();
    let seen: Vec<_> = query
        .iter(&mut world)
        .map(|(entity, value)| {
            value.0 += 1;
            entity
        })
        .collect();

    assert_eq!(seen, vec![active]);
    assert_eq!(world.require::<A>(active).unwrap().0, 6);
    assert_eq!(world.require::<A>(hidden).unwrap().0, 6);
}

#[test]
fn tuple_mut_read_query_marks_changed_for_mut_component() {
    let mut world = World::new();
    let entity = world.spawn((A(3), B(4))).expect("spawn should succeed");
    let changed_a = world.query_state::<(Entity, &A), Changed<A>>();
    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed_a.iter(&world).next().is_none());

    for (a, b) in world.query_state::<(&mut A, &B), ()>().iter(&mut world) {
        a.0 += b.0;
    }

    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed_a.iter(&world).next().is_none());
}

#[test]
fn tuple_double_mut_query_marks_changed_for_both_components() {
    let mut world = World::new();
    let entity = world.spawn((A(1), B(2))).expect("spawn should succeed");
    let changed_a = world.query_state::<(Entity, &A), Changed<A>>();
    let changed_b = world.query_state::<(Entity, &B), Changed<B>>();
    assert!(changed_a.iter(&world).next().is_some());
    assert!(changed_b.iter(&world).next().is_some());
    assert!(changed_a.iter(&world).next().is_none());
    assert!(changed_b.iter(&world).next().is_none());

    for (a, b) in world.query_state::<(&mut A, &mut B), ()>().iter(&mut world) {
        a.0 += 10;
        b.0 += 20;
    }

    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed_b
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
}

#[test]
fn reused_single_mut_query_state_stays_correct_across_archetype_migration() {
    let mut world = World::new();
    let entity = world.spawn(A(10)).expect("spawn should succeed");
    let query = world.query_state::<&mut A, ()>();
    let changed = world.query_state::<(Entity, &A), Changed<A>>();

    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed.iter(&world).next().is_none());

    for value in query.iter(&mut world) {
        value.0 += 1;
    }
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );

    world.insert(entity, B(3)).unwrap();
    for value in query.iter(&mut world) {
        value.0 += 2;
    }
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );

    let _: B = world.remove(entity).unwrap();
    for value in query.iter(&mut world) {
        value.0 += 4;
    }
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(world.require::<A>(entity).unwrap().0, 17);
}

#[test]
fn tuple_mut_read_query_updates_only_mutable_side_change_tracking() {
    let mut world = World::new();
    let entity = world.spawn((A(5), B(7))).expect("spawn should succeed");
    let changed_a = world.query_state::<(Entity, &A), Changed<A>>();
    let changed_b = world.query_state::<(Entity, &B), Changed<B>>();

    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed_b
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed_a.iter(&world).next().is_none());
    assert!(changed_b.iter(&world).next().is_none());

    let (a_added_before, a_changed_before) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist");
    let (b_added_before, b_changed_before) = world
        .__entity_component_ticks::<B>(entity)
        .expect("B ticks should exist");

    for (a, b) in world.query_state::<(&mut A, &B), ()>().iter(&mut world) {
        a.0 += b.0;
    }

    assert_eq!(world.require::<A>(entity).unwrap().0, 12);
    assert_eq!(world.require::<B>(entity).unwrap().0, 7);
    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed_b.iter(&world).next().is_none());

    let (a_added_after, a_changed_after) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist");
    let (b_added_after, b_changed_after) = world
        .__entity_component_ticks::<B>(entity)
        .expect("B ticks should exist");
    assert_eq!(a_added_after, a_added_before);
    assert!(a_changed_after > a_changed_before);
    assert_eq!(b_added_after, b_added_before);
    assert_eq!(b_changed_after, b_changed_before);
}

#[test]
fn tuple_mut_read_query_preserves_pairing_across_component_migration() {
    let mut world = World::new();
    let first = world.spawn((A(1), B(10))).expect("spawn should succeed");
    let second = world.spawn((A(2), B(20))).expect("spawn should succeed");
    let query = world.query_state::<(&mut A, &B), ()>();

    let (first_b_added_before, first_b_changed_before) = world
        .__entity_component_ticks::<B>(first)
        .expect("B ticks should exist");

    for (a, b) in query.iter(&mut world) {
        a.0 += b.0;
    }
    assert_eq!(world.require::<A>(first).unwrap().0, 11);
    assert_eq!(world.require::<A>(second).unwrap().0, 22);

    world.insert(first, C(99)).unwrap();
    for (a, b) in query.iter(&mut world) {
        a.0 += b.0;
    }
    assert_eq!(world.require::<A>(first).unwrap().0, 21);
    assert_eq!(world.require::<A>(second).unwrap().0, 42);

    let _: C = world.remove(first).unwrap();
    for (a, b) in query.iter(&mut world) {
        a.0 += b.0;
    }
    assert_eq!(world.require::<A>(first).unwrap().0, 31);
    assert_eq!(world.require::<A>(second).unwrap().0, 62);
    assert_eq!(world.require::<B>(first).unwrap().0, 10);
    assert_eq!(world.require::<B>(second).unwrap().0, 20);

    let (first_b_added_after, first_b_changed_after) = world
        .__entity_component_ticks::<B>(first)
        .expect("B ticks should exist");
    assert_eq!(first_b_added_after, first_b_added_before);
    assert_eq!(first_b_changed_after, first_b_changed_before);
}

#[test]
fn tuple_double_mut_query_updates_changed_ticks_for_both_components() {
    let mut world = World::new();
    let entity = world.spawn((A(4), B(9))).expect("spawn should succeed");
    let changed_a = world.query_state::<(Entity, &A), Changed<A>>();
    let changed_b = world.query_state::<(Entity, &B), Changed<B>>();

    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed_b
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed_a.iter(&world).next().is_none());
    assert!(changed_b.iter(&world).next().is_none());

    let (a_added_before, a_changed_before) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist");
    let (b_added_before, b_changed_before) = world
        .__entity_component_ticks::<B>(entity)
        .expect("B ticks should exist");

    for (a, b) in world.query_state::<(&mut A, &mut B), ()>().iter(&mut world) {
        a.0 += b.0;
        b.0 += 3;
    }

    assert_eq!(world.require::<A>(entity).unwrap().0, 13);
    assert_eq!(world.require::<B>(entity).unwrap().0, 12);
    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed_b
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );

    let (a_added_after, a_changed_after) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist");
    let (b_added_after, b_changed_after) = world
        .__entity_component_ticks::<B>(entity)
        .expect("B ticks should exist");
    assert_eq!(a_added_after, a_added_before);
    assert!(a_changed_after > a_changed_before);
    assert_eq!(b_added_after, b_added_before);
    assert!(b_changed_after > b_changed_before);
}

#[test]
fn tuple_double_mut_query_preserves_pairing_across_component_migration() {
    let mut world = World::new();
    let first = world.spawn((A(1), B(10))).expect("spawn should succeed");
    let second = world.spawn((A(2), B(20))).expect("spawn should succeed");
    let query = world.query_state::<(&mut A, &mut B), ()>();

    for (a, b) in query.iter(&mut world) {
        let old_a = a.0;
        let old_b = b.0;
        a.0 = old_a + old_b;
        b.0 = old_b - old_a;
    }
    assert_eq!(world.require::<A>(first).unwrap().0, 11);
    assert_eq!(world.require::<B>(first).unwrap().0, 9);
    assert_eq!(world.require::<A>(second).unwrap().0, 22);
    assert_eq!(world.require::<B>(second).unwrap().0, 18);

    world.insert(first, C(99)).unwrap();
    for (a, b) in query.iter(&mut world) {
        let old_a = a.0;
        let old_b = b.0;
        a.0 = old_a + old_b;
        b.0 = old_b - old_a;
    }
    assert_eq!(world.require::<A>(first).unwrap().0, 20);
    assert_eq!(world.require::<B>(first).unwrap().0, -2);
    assert_eq!(world.require::<A>(second).unwrap().0, 40);
    assert_eq!(world.require::<B>(second).unwrap().0, -4);

    let _: C = world.remove(first).unwrap();
    for (a, b) in query.iter(&mut world) {
        let old_a = a.0;
        let old_b = b.0;
        a.0 = old_a + old_b;
        b.0 = old_b - old_a;
    }
    assert_eq!(world.require::<A>(first).unwrap().0, 18);
    assert_eq!(world.require::<B>(first).unwrap().0, -22);
    assert_eq!(world.require::<A>(second).unwrap().0, 36);
    assert_eq!(world.require::<B>(second).unwrap().0, -44);
}

#[test]
fn reused_double_mut_query_state_stays_correct_across_archetype_migration() {
    let mut world = World::new();
    let entity = world.spawn((A(3), B(7))).expect("spawn should succeed");
    let query = world.query_state::<(&mut A, &mut B), ()>();
    let changed_a = world.query_state::<(Entity, &A), Changed<A>>();
    let changed_b = world.query_state::<(Entity, &B), Changed<B>>();

    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed_b
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed_a.iter(&world).next().is_none());
    assert!(changed_b.iter(&world).next().is_none());

    for (a, b) in query.iter(&mut world) {
        a.0 += 1;
        b.0 += 2;
    }
    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed_b
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );

    world.insert(entity, C(1)).unwrap();
    for (a, b) in query.iter(&mut world) {
        a.0 += 1;
        b.0 += 2;
    }
    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed_b
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );

    let _: C = world.remove(entity).unwrap();
    for (a, b) in query.iter(&mut world) {
        a.0 += 1;
        b.0 += 2;
    }
    assert_eq!(
        changed_a
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(
        changed_b
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(world.require::<A>(entity).unwrap().0, 6);
    assert_eq!(world.require::<B>(entity).unwrap().0, 13);
}

#[test]
fn changed_filter_tracks_require_mut_using_archetype_metadata_ticks() {
    let mut world = World::new();
    let entity = world.spawn(A(5)).expect("spawn should succeed");
    let changed = world.query_state::<(Entity, &A), Changed<A>>();

    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed.iter(&world).next().is_none());

    let (added_before, changed_before) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist");

    world.require_mut::<A>(entity).unwrap().0 += 1;

    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );

    let (added_after, changed_after) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist");
    assert_eq!(added_after, added_before);
    assert!(changed_after > changed_before);
}

#[test]
fn optional_forms_remain_correct_after_migration_churn() {
    let mut world = World::new();
    let first = world.spawn((A(1), B(10))).expect("spawn should succeed");
    let second = world.spawn(A(2)).expect("spawn should succeed");
    let third = world.spawn((A(3), C(30))).expect("spawn should succeed");
    let query_a = world.query_state::<(Entity, &A), ()>();
    let query_b = world.query_state::<(Entity, Option<&B>), ()>();
    let query_c = world.query_state::<(Entity, Option<&C>), ()>();

    let mut baseline_a: Vec<_> = query_a
        .iter(&world)
        .map(|(entity, a)| (entity, a.0))
        .collect();
    baseline_a.sort_unstable_by_key(|(entity, _)| *entity);
    let mut baseline_b: Vec<_> = query_b
        .iter(&world)
        .map(|(entity, b)| (entity, b.map(|v| v.0)))
        .collect();
    baseline_b.sort_unstable_by_key(|(entity, _)| *entity);
    let mut baseline_c: Vec<_> = query_c
        .iter(&world)
        .map(|(entity, c)| (entity, c.map(|v| v.0)))
        .collect();
    baseline_c.sort_unstable_by_key(|(entity, _)| *entity);

    assert_eq!(baseline_a, vec![(first, 1), (second, 2), (third, 3),]);
    assert_eq!(
        baseline_b,
        vec![(first, Some(10)), (second, None), (third, None),]
    );
    assert_eq!(
        baseline_c,
        vec![(first, None), (second, None), (third, Some(30)),]
    );

    let _: B = world.remove(first).unwrap();
    world.insert(second, (B(20), C(200))).unwrap();
    world.insert(first, C(100)).unwrap();
    let _: C = world.remove(third).unwrap();
    world.insert(third, B(30)).unwrap();

    let mut after_a: Vec<_> = query_a
        .iter(&world)
        .map(|(entity, a)| (entity, a.0))
        .collect();
    after_a.sort_unstable_by_key(|(entity, _)| *entity);
    let mut after_b: Vec<_> = query_b
        .iter(&world)
        .map(|(entity, b)| (entity, b.map(|v| v.0)))
        .collect();
    after_b.sort_unstable_by_key(|(entity, _)| *entity);
    let mut after_c: Vec<_> = query_c
        .iter(&world)
        .map(|(entity, c)| (entity, c.map(|v| v.0)))
        .collect();
    after_c.sort_unstable_by_key(|(entity, _)| *entity);

    assert_eq!(after_a, vec![(first, 1), (second, 2), (third, 3),]);
    assert_eq!(
        after_b,
        vec![(first, None), (second, Some(20)), (third, Some(30)),]
    );
    assert_eq!(
        after_c,
        vec![(first, Some(100)), (second, Some(200)), (third, None),]
    );
}

#[test]
fn changed_filter_tracks_get_mut_using_archetype_metadata_ticks() {
    let mut world = World::new();
    let entity = world.spawn(A(9)).expect("spawn should succeed");
    let changed = world.query_state::<(Entity, &A), Changed<A>>();

    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed.iter(&world).next().is_none());

    let (added_before, changed_before) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist");
    world.get_mut::<A>(entity).unwrap().0 += 3;

    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );

    let (added_after, changed_after) = world
        .__entity_component_ticks::<A>(entity)
        .expect("A ticks should exist");
    assert_eq!(added_after, added_before);
    assert!(changed_after > changed_before);
}

#[test]
fn added_filter_respects_command_flush_boundary_between_stages() {
    fn queue_spawn_once(mut gate: ResMut<QuerySpawnGate>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.spawn(A(42));
        gate.0 = true;
    }

    fn observe_added(mut query: Query<&A, Added<A>>, mut counts: ResMut<QueryAddedCounts>) {
        counts.0.push(query.iter().count());
    }

    let mut world = World::new();
    world.insert_resource(QuerySpawnGate(false));
    world.insert_resource(QueryAddedCounts(Vec::new()));

    let mut runtime = Runtime::new();
    runtime.add_systems::<QueryUpdate, _, _>(&mut world, queue_spawn_once.in_set(QueueSet));
    runtime.add_systems::<QueryUpdate, _, _>(
        &mut world,
        observe_added.in_set(ObserveSet).after(QueueSet),
    );

    runtime.run_schedule::<QueryUpdate>(&mut world).unwrap();
    runtime.run_schedule::<QueryUpdate>(&mut world).unwrap();

    assert_eq!(world.resource::<QueryAddedCounts>().unwrap().0, vec![1, 0]);
}
