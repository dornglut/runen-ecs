use runen_ecs::prelude::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
struct Position(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
struct Velocity(i32);

#[test]
fn mutable_pair_updates_every_row_and_rebuilds_both_component_indexes() {
    let mut world = World::new();
    world.ensure_component_index::<Position, i32>(|value| value.0);
    world.ensure_component_index::<Velocity, i32>(|value| value.0);

    // Both entities belong to one archetype, so both changed-tick pointers for
    // each column must remain valid through the same contiguous projection.
    let first = world.spawn((Position(1), Velocity(10))).unwrap();
    let second = world.spawn((Position(2), Velocity(20))).unwrap();
    assert_eq!(world.find_entity_by_index::<Position, i32>(&1), Some(first));
    assert_eq!(
        world.find_entity_by_index::<Position, i32>(&2),
        Some(second)
    );
    assert_eq!(
        world.find_entity_by_index::<Velocity, i32>(&10),
        Some(first)
    );
    assert_eq!(
        world.find_entity_by_index::<Velocity, i32>(&20),
        Some(second)
    );

    let before = [
        world.__entity_component_ticks::<Position>(first).unwrap(),
        world.__entity_component_ticks::<Velocity>(first).unwrap(),
        world.__entity_component_ticks::<Position>(second).unwrap(),
        world.__entity_component_ticks::<Velocity>(second).unwrap(),
    ];
    let changed_positions = world.query_filtered::<(Entity, &Position), Changed<Position>>();
    let changed_velocities = world.query_filtered::<(Entity, &Velocity), Changed<Velocity>>();
    assert_eq!(changed_positions.iter(&world).count(), 2);
    assert_eq!(changed_velocities.iter(&world).count(), 2);
    assert_eq!(changed_positions.iter(&world).count(), 0);
    assert_eq!(changed_velocities.iter(&world).count(), 0);

    let query = world.query::<(&mut Position, &mut Velocity)>();
    {
        let mut segments = query.try_contiguous_segments(&mut world).unwrap();
        assert_eq!(segments.len(), 1);
        let mut segment = segments.next().unwrap();
        let (positions, velocities) = segment.component_pair_mut::<Position, Velocity>().unwrap();
        assert_eq!(positions.len(), 2);
        assert_eq!(velocities.len(), 2);
        for (position, velocity) in positions.iter_mut().zip(velocities.iter_mut()) {
            assert_eq!(velocity.0, position.0 * 10);
            position.0 += 10;
            velocity.0 += 100;
        }
    }

    assert_eq!(changed_positions.iter(&world).count(), 2);
    assert_eq!(changed_velocities.iter(&world).count(), 2);
    let after = [
        world.__entity_component_ticks::<Position>(first).unwrap(),
        world.__entity_component_ticks::<Velocity>(first).unwrap(),
        world.__entity_component_ticks::<Position>(second).unwrap(),
        world.__entity_component_ticks::<Velocity>(second).unwrap(),
    ];
    for (before_row, after_row) in before.into_iter().zip(after) {
        assert_eq!(before_row.0, after_row.0);
        assert!(after_row.1 > before_row.1);
    }

    // Re-querying both indexes must discard the stale keys and use the
    // updated values, not just the first mutable column's updated values.
    assert_eq!(world.find_entity_by_index::<Position, i32>(&1), None);
    assert_eq!(world.find_entity_by_index::<Position, i32>(&2), None);
    assert_eq!(world.find_entity_by_index::<Velocity, i32>(&10), None);
    assert_eq!(world.find_entity_by_index::<Velocity, i32>(&20), None);
    assert_eq!(
        world.find_entity_by_index::<Position, i32>(&11),
        Some(first)
    );
    assert_eq!(
        world.find_entity_by_index::<Position, i32>(&12),
        Some(second)
    );
    assert_eq!(
        world.find_entity_by_index::<Velocity, i32>(&110),
        Some(first)
    );
    assert_eq!(
        world.find_entity_by_index::<Velocity, i32>(&120),
        Some(second)
    );
}
