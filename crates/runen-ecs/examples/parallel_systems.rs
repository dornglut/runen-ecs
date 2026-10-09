use runen_ecs::prelude::*;
use runen_ecs::{RuntimeError, WorldMut};

#[derive(ScheduleLabel)]
struct Update;

#[derive(SystemSet)]
struct Spawn;

#[derive(SystemSet)]
struct Simulation;

#[derive(Component)]
struct Position(i32);

#[derive(Component)]
struct Velocity(i32);

#[derive(Resource)]
struct Frames(u32);

fn queue_arrival(mut commands: Commands) {
    // Recorded on the worker; published before Simulation because the
    // explicit Spawn -> Simulation dependency creates a visibility frontier.
    commands.spawn((Position(10), Velocity(4)));
}

fn advance_positions(mut positions: Query<&mut Position>) {
    for position in positions.iter() {
        position.0 += 1;
    }
}

fn nudge_positions(mut positions: Query<&mut Position>) {
    // Conflicts with advance_positions: both write Position, so they cannot
    // overlap. They deliberately commute; conflict is not semantic precedence.
    for position in positions.iter() {
        position.0 += 10;
    }
}

fn advance_velocities(mut velocities: Query<&mut Velocity>) {
    // Independent Velocity writes may overlap with Position mutations.
    for velocity in velocities.iter() {
        velocity.0 += 2;
    }
}

fn finish_frame(mut world: WorldMut) {
    // Full World access is exclusive and stays on the invoking thread.
    // Narrow Query/ResMut capabilities should be preferred when sufficient.
    world.resource_mut::<Frames>().unwrap().0 += 1;
}

fn main() -> Result<(), RuntimeError> {
    let mut world = World::new();
    world.spawn((Position(0), Velocity(1)))?;
    world.insert_resource(Frames(0));

    let mut runtime = Runtime::new();
    runtime.add_systems(
        Update,
        (
            queue_arrival.in_set(Spawn),
            advance_positions.in_set(Simulation).after(Spawn),
            advance_velocities.in_set(Simulation).after(Spawn),
            nudge_positions.in_set(Simulation).after(Spawn),
            finish_frame.on_invoker_thread().after(Simulation),
        ),
    )?;

    // Capacity is an upper bound, not a guarantee that every system overlaps.
    // Each Query still iterates serially *within* its own system.
    runtime.run_schedule_parallel::<Update>(&mut world, 2)?;

    // The spawned entity became visible before Simulation; both position
    // writers ran without overlapping, and the exclusive system ran last.
    let mut pairs = world
        .query::<(&Position, &Velocity)>()
        .iter(&world)
        .map(|(position, velocity)| (position.0, velocity.0))
        .collect::<Vec<_>>();
    pairs.sort_unstable();
    assert_eq!(pairs, [(11, 3), (21, 6)]);
    assert_eq!(world.resource::<Frames>().unwrap().0, 1);

    println!("parallel schedule: 2 entities updated, 1 exclusive frame fence");
    Ok(())
}
