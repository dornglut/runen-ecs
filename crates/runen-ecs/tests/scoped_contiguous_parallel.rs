//! Public-API proof that explicit scoped threads may mutate only disjoint
//! typed contiguous query slices while WorldMut stays on the invoking thread.
use runen_ecs::WorldMut;
use runen_ecs::prelude::*;

#[derive(Component)]
struct Position(i64);

#[derive(Component)]
struct Velocity(i64);

#[derive(Component)]
struct Extra;

#[derive(Component)]
struct Skip;

#[derive(ScheduleLabel)]
struct Update;

fn make_world() -> (World, Vec<(Entity, i64, bool)>) {
    let mut world = World::new();
    world.ensure_component_index::<Position, i64>(|position| position.0);

    let mut expected = Vec::new();
    for value in 0..289_i64 {
        let skip = value % 19 == 0;
        let extra = value % 2 == 0;
        let entity = match (skip, extra) {
            (false, false) => world.spawn((Position(value), Velocity(4096))),
            (false, true) => world.spawn((Position(value), Velocity(4096), Extra)),
            (true, false) => world.spawn((Position(value), Velocity(4096), Skip)),
            (true, true) => world.spawn((Position(value), Velocity(4096), Skip, Extra)),
        }
        .unwrap();
        expected.push((entity, value, skip));
    }
    (world, expected)
}

fn integrate_serial(mut query: Query<(&mut Position, &Velocity), Without<Skip>>) {
    for (position, velocity) in query.iter() {
        position.0 += velocity.0;
    }
}

fn integrate_scoped(mut world: WorldMut) {
    let query = world.query_filtered::<(&mut Position, &Velocity), Without<Skip>>();
    for mut segment in query.try_contiguous_segments(&mut *world).unwrap() {
        // This records conservative per-row change events and invalidates
        // Position indexes before mutable payload slices are exposed.
        let (positions, velocities) = segment
            .component_pair_mut_shared::<Position, Velocity>()
            .unwrap();

        // Workers own only disjoint typed slice borrows. WorldMut, World,
        // QueryState and ContiguousSegment remain on the invoking thread.
        std::thread::scope(|scope| {
            for (positions, velocities) in positions.chunks_mut(37).zip(velocities.chunks(37)) {
                scope.spawn(move || {
                    for (position, velocity) in positions.iter_mut().zip(velocities.iter()) {
                        position.0 += velocity.0;
                    }
                });
            }
        });
    }
}

#[test]
fn scoped_contiguous_chunks_match_serial_world_observation() {
    let (mut reference, reference_rows) = make_world();
    let (mut candidate, candidate_rows) = make_world();
    let eligible = reference_rows.iter().filter(|(_, _, skip)| !skip).count();

    let reference_changed = reference.query_filtered::<&Position, Changed<Position>>();
    let candidate_changed = candidate.query_filtered::<&Position, Changed<Position>>();
    assert_eq!(
        reference_changed.iter(&reference).count(),
        reference_rows.len()
    );
    assert_eq!(
        candidate_changed.iter(&candidate).count(),
        candidate_rows.len()
    );
    assert_eq!(reference_changed.iter(&reference).count(), 0);
    assert_eq!(candidate_changed.iter(&candidate).count(), 0);

    let reference_before = reference.current_change_cursor();
    let candidate_before = candidate.current_change_cursor();

    let mut serial = Runtime::new();
    serial.add_systems(Update, integrate_serial).unwrap();
    serial.run_schedule::<Update>(&mut reference).unwrap();

    let mut parallel = Runtime::new();
    parallel
        .add_systems(Update, integrate_scoped.on_invoker_thread())
        .unwrap();
    parallel
        .run_schedule_parallel::<Update>(&mut candidate, 4)
        .unwrap();

    for ((reference_entity, value, skip), (candidate_entity, _, _)) in
        reference_rows.iter().zip(candidate_rows.iter())
    {
        let expected = if *skip { *value } else { *value + 4096 };
        assert_eq!(
            reference.require::<Position>(*reference_entity).unwrap().0,
            expected
        );
        assert_eq!(
            candidate.require::<Position>(*candidate_entity).unwrap().0,
            expected
        );
    }

    assert_eq!(reference_changed.iter(&reference).count(), eligible);
    assert_eq!(candidate_changed.iter(&candidate).count(), eligible);
    assert_eq!(
        reference.current_change_cursor().tick() - reference_before.tick(),
        eligible as u64
    );
    assert_eq!(
        candidate.current_change_cursor().tick() - candidate_before.tick(),
        eligible as u64
    );

    // Index maintenance must not be bypassed by the scoped slice work.
    assert_eq!(reference.find_entity_by_index::<Position, i64>(&41), None);
    assert_eq!(candidate.find_entity_by_index::<Position, i64>(&41), None);
    assert_eq!(
        reference.find_entity_by_index::<Position, i64>(&4137),
        Some(reference_rows[41].0)
    );
    assert_eq!(
        candidate.find_entity_by_index::<Position, i64>(&4137),
        Some(candidate_rows[41].0)
    );
}
