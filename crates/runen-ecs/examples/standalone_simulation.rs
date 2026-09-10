use runen_ecs::prelude::*;
use runen_ecs::{RuntimeError, WorldMut};

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Copy, Clone)]
struct Spawn;

impl SystemSet for Spawn {
    fn name() -> &'static str {
        "Spawn"
    }
}

#[derive(Copy, Clone)]
struct Observe;

impl SystemSet for Observe {
    fn name() -> &'static str {
        "Observe"
    }
}

#[derive(Debug, Component)]
struct Position(i32);

#[derive(Debug, Component)]
struct Velocity(i32);

#[derive(Debug, Component)]
struct NewArrival;

#[derive(Debug, Resource)]
struct SpawnedCount(usize);

#[derive(Debug, Resource)]
struct Frame(usize);

fn queue_arrival(mut commands: Commands) {
    commands.spawn((Position(10), Velocity(2), NewArrival));
}

fn integrate(mut query: Query<(&mut Position, &Velocity)>) {
    for (position, velocity) in query.iter() {
        position.0 += velocity.0;
    }
}

fn observe_arrivals(
    mut query: Query<&Position, With<NewArrival>>,
    mut spawned: ResMut<SpawnedCount>,
) {
    spawned.0 = query.iter().count();
}

fn advance_frame(mut world: WorldMut) {
    world.resource_mut::<Frame>().unwrap().0 += 1;
}

fn main() -> Result<(), RuntimeError> {
    let mut world = World::new();
    world.insert_resource(SpawnedCount(0));
    world.insert_resource(Frame(0));
    world.spawn((Position(0), Velocity(1))).unwrap();

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (
            queue_arrival.in_set(Spawn),
            integrate,
            observe_arrivals.in_set(Observe).after(Spawn),
            advance_frame.after(Observe),
        ),
    );

    let mut boundaries = Vec::new();
    runtime.run_schedule_with_deferred_apply_boundary::<Update, _, _>(
        &mut world,
        |boundary, world| {
            boundaries.push((
                boundary.index(),
                world.query_state::<&Position, ()>().iter(world).count(),
            ));
            Ok::<(), std::convert::Infallible>(())
        },
    )?;

    assert_eq!(boundaries, vec![(0, 2), (1, 2), (2, 2)]);
    assert_eq!(world.resource::<SpawnedCount>().unwrap().0, 1);
    assert_eq!(world.resource::<Frame>().unwrap().0, 1);
    Ok(())
}
