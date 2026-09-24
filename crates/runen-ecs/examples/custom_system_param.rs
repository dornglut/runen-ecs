use runen_ecs::prelude::*;

#[derive(Component)]
struct Position(f32);

#[derive(Component)]
struct Velocity(f32);

#[derive(Resource)]
struct DeltaTime(f32);

#[derive(runen_ecs::SystemParam)]
struct MotionParams<'w, 's> {
    // Query SystemParams require their query-specification type to be 'static.
    // The component references yielded while this system runs are still scoped
    // to that invocation.
    bodies: Query<'w, 's, (&'static mut Position, &'static Velocity)>,
    delta_time: Res<'w, DeltaTime>,
}

#[derive(ScheduleLabel)]
struct Update;

fn integrate(mut params: MotionParams<'_, '_>) {
    for (position, velocity) in params.bodies.iter() {
        position.0 += velocity.0 * params.delta_time.0;
    }
}

fn main() {
    let mut world = World::new();
    let entity = world
        .spawn((Position(1.0), Velocity(4.0)))
        .expect("entity should spawn");
    world.insert_resource(DeltaTime(0.5));

    let mut runtime = Runtime::new();

    // The derived group is an ordinary system parameter. Normal registration
    // proves transferability from its child parameters; the derive does not
    // bypass any child mobility requirement.
    runtime
        .add_systems(Update, integrate)
        .expect("transferable parameter group should register normally");
    runtime
        .run_schedule::<Update>(&mut world)
        .expect("schedule should run");

    let position = world
        .get::<Position>(entity)
        .expect("entity should still have Position")
        .0;
    assert_eq!(position, 3.0);

    println!("integrated position: {position}");
}
