use runen_ecs::RuntimeError;
use runen_ecs::prelude::*;

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
struct Simulate;
impl SystemSet for Simulate {
    fn name() -> &'static str {
        "Simulate"
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

fn advance_frame(mut frame: ResMut<Frame>) {
    frame.0 += 1;
}

fn main() -> Result<(), RuntimeError> {
    let mut world = World::new();
    world.insert_resource(SpawnedCount(0));
    world.insert_resource(Frame(0));
    world.spawn((Position(0), Velocity(1))).unwrap();

    let mut runtime = Runtime::new();
    runtime.add_systems(
        Update,
        (
            queue_arrival.in_set(Spawn),
            integrate.in_set(Simulate).after(Spawn),
            observe_arrivals.in_set(Observe).after(Simulate),
            advance_frame.after(Observe),
        ),
    )?;

    runtime.run_schedule::<Update>(&mut world)?;

    let arrival = world
        .query_state::<&Position, With<NewArrival>>()
        .single(&world)
        .expect("one arrival should exist");
    assert_eq!(arrival.0, 12);
    assert_eq!(world.resource::<SpawnedCount>().unwrap().0, 1);
    assert_eq!(world.resource::<Frame>().unwrap().0, 1);

    println!(
        "frame 1: one new arrival integrated to position {}",
        arrival.0
    );
    Ok(())
}
