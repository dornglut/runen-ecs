use runen_ecs::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
struct A(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
struct B(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
struct Extra;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
struct Selected;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
struct Hidden;

static B_INDEX_EXTRACTS: AtomicUsize = AtomicUsize::new(0);

fn extract_b(value: &B) -> i32 {
    B_INDEX_EXTRACTS.fetch_add(1, Ordering::SeqCst);
    value.0
}

#[test]
fn shared_segments_are_contiguous_aligned_filtered_and_unordered() {
    let mut world = World::new();
    let first = world.spawn((A(1), B(10), Selected)).unwrap();
    let second = world.spawn((A(2), B(20), Selected, Extra)).unwrap();
    let hidden = world.spawn((A(3), B(30), Selected, Hidden)).unwrap();

    let query = world.query_filtered::<(&A, &B), (With<Selected>, Without<Hidden>)>();
    let segments = query.try_contiguous_segments(&world).unwrap();
    assert_eq!(segments.len(), 2);

    let mut pairs = Vec::new();
    for segment in segments {
        let entities = segment.entities();
        let (a_values, b_values) = segment.component_pair::<A, B>().unwrap();
        assert_eq!(segment.len(), entities.len());
        assert_eq!(entities.len(), a_values.len());
        assert_eq!(a_values.len(), b_values.len());
        for ((entity, a), b) in entities.iter().zip(a_values).zip(b_values) {
            assert_eq!(b.0, a.0 * 10);
            pairs.push((*entity, a.0, b.0));
        }
    }

    pairs.sort_unstable_by_key(|(entity, _, _)| *entity);
    assert_eq!(pairs, vec![(first, 1, 10), (second, 2, 20)]);
    assert!(!pairs.iter().any(|(entity, _, _)| *entity == hidden));

    let entity_query = world.query::<(Entity, &A)>();
    let mut entity_values = Vec::new();
    for segment in entity_query.try_contiguous_segments(&world).unwrap() {
        let entities = segment.entities();
        let values = segment.component::<A>().unwrap();
        entity_values.extend(entities.iter().copied().zip(values.iter().map(|a| a.0)));
    }
    entity_values.sort_unstable_by_key(|(entity, _)| *entity);
    assert_eq!(entity_values, vec![(first, 1), (second, 2), (hidden, 3)]);
}

#[test]
fn unsupported_filters_optional_shapes_and_duplicate_types_fail_explicitly() {
    let mut world = World::new();
    world.spawn((A(1), B(2))).unwrap();

    let added = world.query_filtered::<&A, Added<A>>();
    assert!(matches!(
        added.try_contiguous_segments(&mut world),
        Err(ContiguousQueryError::UnsupportedFilterShape)
    ));

    let changed = world.query_filtered::<&A, Changed<A>>();
    assert!(matches!(
        changed.try_contiguous_segments(&mut world),
        Err(ContiguousQueryError::UnsupportedFilterShape)
    ));

    let optional = world.query::<Option<&B>>();
    assert!(matches!(
        optional.try_contiguous_segments(&mut world),
        Err(ContiguousQueryError::UnsupportedQueryShape)
    ));

    let duplicate_shared = world.query::<(&A, &A)>();
    assert!(matches!(
        duplicate_shared.try_contiguous_segments(&mut world),
        Err(ContiguousQueryError::AliasedComponentType)
    ));
}

#[test]
fn mutable_shared_segments_preserve_ticks_and_invalidate_only_mutable_indexes() {
    let mut world = World::new();
    world.ensure_component_index::<A, i32>(|value| value.0);
    world.ensure_component_index::<B, i32>(extract_b);
    let first = world.spawn((A(1), B(10))).unwrap();
    let second = world.spawn((A(2), B(20), Extra)).unwrap();
    assert_eq!(world.find_entity_by_index::<A, i32>(&1), Some(first));
    assert_eq!(world.find_entity_by_index::<A, i32>(&2), Some(second));
    assert_eq!(world.find_entity_by_index::<B, i32>(&10), Some(first));
    assert_eq!(world.find_entity_by_index::<B, i32>(&20), Some(second));
    B_INDEX_EXTRACTS.store(0, Ordering::SeqCst);

    let changed_a = world.query_filtered::<(Entity, &A), Changed<A>>();
    let added_a = world.query_filtered::<(Entity, &A), Added<A>>();
    assert_eq!(changed_a.iter(&world).count(), 2);
    assert_eq!(changed_a.iter(&world).count(), 0);
    assert_eq!(added_a.iter(&world).count(), 2);
    assert_eq!(added_a.iter(&world).count(), 0);
    let (first_a_added, first_a_changed) = world.__entity_component_ticks::<A>(first).unwrap();
    let (first_b_added, first_b_changed) = world.__entity_component_ticks::<B>(first).unwrap();
    let (second_a_added, second_a_changed) = world.__entity_component_ticks::<A>(second).unwrap();
    let (second_b_added, second_b_changed) = world.__entity_component_ticks::<B>(second).unwrap();
    let cursor_before = world.current_change_cursor();

    let query = world.query::<(&mut A, &B)>();
    for mut segment in query.try_contiguous_segments(&mut world).unwrap() {
        let (a_values, b_values) = segment.component_pair_mut_shared::<A, B>().unwrap();
        for (a, b) in a_values.iter_mut().zip(b_values) {
            a.0 += b.0;
        }
    }

    assert!(world.component_changed_since::<A>(cursor_before).unwrap());
    assert_eq!(changed_a.iter(&world).count(), 2);
    assert_eq!(added_a.iter(&world).count(), 0);
    assert_eq!(world.require::<A>(first).unwrap().0, 11);
    assert_eq!(world.require::<A>(second).unwrap().0, 22);
    assert_eq!(world.find_entity_by_index::<A, i32>(&1), None);
    assert_eq!(world.find_entity_by_index::<A, i32>(&11), Some(first));
    assert_eq!(world.find_entity_by_index::<A, i32>(&22), Some(second));
    assert_eq!(B_INDEX_EXTRACTS.load(Ordering::SeqCst), 0);
    assert_eq!(world.find_entity_by_index::<B, i32>(&10), Some(first));
    assert_eq!(B_INDEX_EXTRACTS.load(Ordering::SeqCst), 0);

    let (first_a_added_after, first_a_changed_after) =
        world.__entity_component_ticks::<A>(first).unwrap();
    let (first_b_added_after, first_b_changed_after) =
        world.__entity_component_ticks::<B>(first).unwrap();
    let (second_a_added_after, second_a_changed_after) =
        world.__entity_component_ticks::<A>(second).unwrap();
    let (second_b_added_after, second_b_changed_after) =
        world.__entity_component_ticks::<B>(second).unwrap();
    assert_eq!(first_a_added_after, first_a_added);
    assert!(first_a_changed_after > first_a_changed);
    assert_eq!(first_b_added_after, first_b_added);
    assert_eq!(first_b_changed_after, first_b_changed);
    assert_eq!(second_a_added_after, second_a_added);
    assert!(second_a_changed_after > second_a_changed);
    assert_eq!(second_b_added_after, second_b_added);
    assert_eq!(second_b_changed_after, second_b_changed);
}

#[test]
fn mutable_mutable_segments_track_both_columns_and_reused_states_see_new_archetypes() {
    let mut world = World::new();
    let first = world.spawn((A(1), B(2))).unwrap();
    let query = world.query::<(&mut A, &mut B)>();
    assert_eq!(query.try_contiguous_segments(&mut world).unwrap().len(), 1);

    let second = world.spawn((A(3), B(4), Extra)).unwrap();
    let ticks_before = [
        world.__entity_component_ticks::<A>(first).unwrap(),
        world.__entity_component_ticks::<B>(first).unwrap(),
        world.__entity_component_ticks::<A>(second).unwrap(),
        world.__entity_component_ticks::<B>(second).unwrap(),
    ];
    let changed_a = world.query_filtered::<(Entity, &A), Changed<A>>();
    let changed_b = world.query_filtered::<(Entity, &B), Changed<B>>();
    assert_eq!(changed_a.iter(&world).count(), 2);
    assert_eq!(changed_b.iter(&world).count(), 2);
    assert_eq!(changed_a.iter(&world).count(), 0);
    assert_eq!(changed_b.iter(&world).count(), 0);

    for mut segment in query.try_contiguous_segments(&mut world).unwrap() {
        let (a_values, b_values) = segment.component_pair_mut::<A, B>().unwrap();
        for (a, b) in a_values.iter_mut().zip(b_values.iter_mut()) {
            a.0 += b.0;
            b.0 += 5;
        }
    }

    assert_eq!(changed_a.iter(&world).count(), 2);
    assert_eq!(changed_b.iter(&world).count(), 2);
    let ticks_after = [
        world.__entity_component_ticks::<A>(first).unwrap(),
        world.__entity_component_ticks::<B>(first).unwrap(),
        world.__entity_component_ticks::<A>(second).unwrap(),
        world.__entity_component_ticks::<B>(second).unwrap(),
    ];
    for (before, after) in ticks_before.into_iter().zip(ticks_after) {
        assert_eq!(before.0, after.0);
        assert!(after.1 > before.1);
    }
    assert_eq!(world.require::<A>(first).unwrap().0, 3);
    assert_eq!(world.require::<B>(first).unwrap().0, 7);
    assert_eq!(world.require::<A>(second).unwrap().0, 7);
    assert_eq!(world.require::<B>(second).unwrap().0, 9);
}

#[test]
fn mutable_segments_record_only_rows_whose_mutable_slice_is_exposed() {
    let mut world = World::new();
    let first = world.spawn(A(1)).unwrap();
    let second = world.spawn((A(2), Extra)).unwrap();
    let before = [
        world.__entity_component_ticks::<A>(first).unwrap(),
        world.__entity_component_ticks::<A>(second).unwrap(),
    ];
    let changed = world.query_filtered::<(Entity, &A), Changed<A>>();
    assert_eq!(changed.iter(&world).count(), 2);
    assert_eq!(changed.iter(&world).count(), 0);

    let query = world.query::<(Entity, &mut A)>();
    let mut segments = query.try_contiguous_segments(&mut world).unwrap();
    let mut first_segment = segments.next().unwrap();
    let selected_entity = first_segment.entities()[0];
    assert_eq!(
        first_segment.component::<A>().unwrap().len(),
        first_segment.len()
    );
    {
        let values = first_segment.component_mut::<A>().unwrap();
        values[0].0 += 10;
    }
    drop(first_segment);
    drop(segments);

    assert_eq!(changed.iter(&world).count(), 1);
    let after = [
        world.__entity_component_ticks::<A>(first).unwrap(),
        world.__entity_component_ticks::<A>(second).unwrap(),
    ];
    for ((entity, before_ticks), after_ticks) in [first, second].into_iter().zip(before).zip(after)
    {
        assert_eq!(before_ticks.0, after_ticks.0);
        if entity == selected_entity {
            assert!(after_ticks.1 > before_ticks.1);
        } else {
            assert_eq!(after_ticks.1, before_ticks.1);
        }
    }
}

#[test]
fn query_state_rebinds_to_a_new_world_lineage() {
    let first_world = World::new();
    let query = first_world.query::<&mut A>();
    let mut second_world = World::new();
    let entity = second_world.spawn(A(4)).unwrap();
    let cursor_before = second_world.current_change_cursor();

    for mut segment in query.try_contiguous_segments(&mut second_world).unwrap() {
        segment.component_mut::<A>().unwrap()[0].0 += 1;
    }

    assert_eq!(second_world.require::<A>(entity).unwrap().0, 5);
    assert!(
        second_world
            .component_changed_since::<A>(cursor_before)
            .unwrap()
    );
}
