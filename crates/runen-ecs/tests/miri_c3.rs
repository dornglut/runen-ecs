use runen_ecs::{Component, Query, ResMut, Resource, Runtime, ScheduleLabel, World};

#[derive(Debug, Copy, Clone, Component)]
struct A(i32);

#[derive(Debug, Copy, Clone, Component)]
struct B(i32);

#[derive(Debug, Copy, Clone, Component)]
struct Extra;

#[derive(Debug, Copy, Clone, Component)]
struct ZeroA;

#[derive(Debug, Copy, Clone, Component)]
struct ZeroB;

#[derive(Debug, Copy, Clone, Resource)]
struct ResourceA(i32);

#[derive(Debug, Copy, Clone, Resource)]
struct ResourceB(i32);

#[derive(Copy, Clone)]
struct C3;

impl ScheduleLabel for C3 {
    fn name() -> &'static str {
        "C3Miri"
    }
}

fn world_with_two_entities() -> World {
    let mut world = World::new();
    world.spawn((A(1), B(10))).unwrap();
    world.spawn((A(2), B(20))).unwrap();
    world
}

#[test]
fn mutable_query_items_remain_unique_across_iterator_advancement() {
    let mut world = world_with_two_entities();
    let query = world.query::<&mut A>();
    let mut iter = query.iter(&mut world);
    let first = iter.next().unwrap();
    let second = iter.next().unwrap();
    first.0 += 10;
    second.0 += 20;
    assert!(iter.next().is_none());
}

#[test]
fn mutable_tuple_items_remain_disjoint_across_iterator_advancement() {
    let mut world = world_with_two_entities();
    let query = world.query::<(&mut A, &mut B)>();
    let mut iter = query.iter(&mut world);
    let (first_a, first_b) = iter.next().unwrap();
    let (second_a, second_b) = iter.next().unwrap();
    first_a.0 += 1;
    first_b.0 += 2;
    second_a.0 += 3;
    second_b.0 += 4;
    assert!(iter.next().is_none());
}

#[test]
fn query_and_disjoint_query_can_retain_items_together() {
    fn system(mut query_a: Query<&mut A>, mut query_b: Query<&mut B>) {
        let mut a_iter = query_a.iter();
        let first_a = a_iter.next().unwrap();
        let mut b_iter = query_b.iter();
        let first_b = b_iter.next().unwrap();
        first_a.0 += 1;
        first_b.0 += 2;
    }

    let mut world = world_with_two_entities();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(C3, system);
    runtime.run_schedule::<C3>(&mut world).unwrap();
}

#[test]
fn query_item_and_resource_mutation_can_be_live_together() {
    fn system(mut query: Query<&mut A>, mut resource: ResMut<ResourceB>) {
        let mut iter = query.iter();
        let item = iter.next().unwrap();
        drop(iter);
        resource.0 += 1;
        item.0 += resource.0;
    }

    let mut world = world_with_two_entities();
    world.insert_resource(ResourceB(4));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(C3, system);
    runtime.run_schedule::<C3>(&mut world).unwrap();
}

#[test]
fn query_scratch_is_recycled_after_early_drop_with_live_direct_iterators() {
    let world = world_with_two_entities();
    let query = world.query::<&A>();

    let mut first = query.iter(&world);
    let mut second = query.iter(&world);
    assert_eq!(first.next().unwrap().0, 1);
    assert_eq!(second.next().unwrap().0, 1);
    drop(first);
    drop(second);

    assert_eq!(query.iter(&world).count(), 2);
}

#[test]
fn resource_payloads_survive_other_resource_mutation_bookkeeping() {
    fn system(mut first: ResMut<ResourceA>, mut second: ResMut<ResourceB>) {
        let first_value: &mut ResourceA = &mut first;
        let second_value: &mut ResourceB = &mut second;
        second_value.0 += 1;
        first_value.0 += second_value.0;
    }

    let mut world = World::new();
    world.insert_resource(ResourceA(1));
    world.insert_resource(ResourceB(2));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(C3, system);
    runtime.run_schedule::<C3>(&mut world).unwrap();
}

#[test]
fn migrated_archetype_query_still_yields_unique_mutable_items() {
    let mut world = World::new();
    let first_entity = world.spawn(A(1)).unwrap();
    let second_entity = world.spawn(A(2)).unwrap();
    world.insert(first_entity, B(10)).unwrap();
    world.insert(second_entity, B(20)).unwrap();

    let query = world.query::<&mut A>();
    let mut iter = query.iter(&mut world);
    let first = iter.next().unwrap();
    let second = iter.next().unwrap();
    first.0 += 10;
    second.0 += 20;
    assert!(iter.next().is_none());
}

#[test]
fn mutable_contiguous_segments_remain_disjoint_while_slices_are_retained() {
    let mut world = World::new();
    world.spawn((A(1), B(10))).unwrap();
    world.spawn((A(2), B(20), Extra)).unwrap();

    let query = world.query::<(&mut A, &mut B)>();
    let mut segments = query.try_contiguous_segments(&mut world).unwrap();
    assert_eq!(segments.len(), 2);

    let mut first_segment = segments.next().unwrap();
    let (first_a, first_b) = first_segment.component_pair_mut::<A, B>().unwrap();
    let mut second_segment = segments.next().unwrap();
    let (second_a, second_b) = second_segment.component_pair_mut::<A, B>().unwrap();

    first_a[0].0 += 1;
    first_b[0].0 += 2;
    second_a[0].0 += 3;
    second_b[0].0 += 4;
    assert!(segments.next().is_none());

    let mut zst_world = World::new();
    zst_world.spawn((ZeroA, ZeroB)).unwrap();
    let zst_query = zst_world.query::<(&mut ZeroA, &mut ZeroB)>();
    let mut zst_segments = zst_query.try_contiguous_segments(&mut zst_world).unwrap();
    let mut zst_segment = zst_segments.next().unwrap();
    let (zero_a, zero_b) = zst_segment.component_pair_mut::<ZeroA, ZeroB>().unwrap();
    zero_a[0] = ZeroA;
    zero_b[0] = ZeroB;
}

#[test]
fn mutable_contiguous_multirow_metadata_and_entity_payload_remain_disjoint() {
    let mut world = World::new();
    let first = world.spawn((A(1), B(10))).unwrap();
    let second = world.spawn((A(2), B(20))).unwrap();
    let cursor = world.current_change_cursor();

    let tuple_query = world.query::<(&mut A, &mut B)>();
    {
        let mut segments = tuple_query.try_contiguous_segments(&mut world).unwrap();
        assert_eq!(segments.len(), 1);
        let mut segment = segments.next().unwrap();
        let (a_values, b_values) = segment.component_pair_mut::<A, B>().unwrap();
        assert_eq!(a_values.len(), 2);
        assert_eq!(b_values.len(), 2);
        for (a, b) in a_values.iter_mut().zip(b_values.iter_mut()) {
            a.0 += 1;
            b.0 += 2;
        }
    }
    assert!(world.component_changed_since::<A>(cursor).unwrap());
    assert!(world.component_changed_since::<B>(cursor).unwrap());
    assert_eq!(world.require::<A>(first).unwrap().0, 2);
    assert_eq!(world.require::<A>(second).unwrap().0, 3);
    assert_eq!(world.require::<B>(first).unwrap().0, 12);
    assert_eq!(world.require::<B>(second).unwrap().0, 22);

    let entity_query = world.query::<(runen_ecs::Entity, &mut A)>();
    {
        let mut segments = entity_query.try_contiguous_segments(&mut world).unwrap();
        assert_eq!(segments.len(), 1);
        let mut segment = segments.next().unwrap();
        let (entities, values) = segment.entity_component_mut::<A>().unwrap();
        assert_eq!(entities.len(), values.len());
        for (entity, value) in entities.iter().zip(values.iter_mut()) {
            value.0 += if *entity == first { 10 } else { 20 };
        }
    }
    assert_eq!(world.require::<A>(first).unwrap().0, 12);
    assert_eq!(world.require::<A>(second).unwrap().0, 23);

    let mut zst_world = World::new();
    zst_world.spawn((ZeroA, ZeroB)).unwrap();
    zst_world.spawn((ZeroA, ZeroB)).unwrap();
    let zst_query = zst_world.query::<(&mut ZeroA, &mut ZeroB)>();
    let mut zst_segments = zst_query.try_contiguous_segments(&mut zst_world).unwrap();
    let mut zst_segment = zst_segments.next().unwrap();
    let (zero_a, zero_b) = zst_segment.component_pair_mut::<ZeroA, ZeroB>().unwrap();
    assert_eq!(zero_a.len(), 2);
    assert_eq!(zero_b.len(), 2);
    for (a, b) in zero_a.iter_mut().zip(zero_b.iter_mut()) {
        *a = ZeroA;
        *b = ZeroB;
    }
}
