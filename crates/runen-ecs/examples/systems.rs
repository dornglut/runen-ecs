use runen_ecs::prelude::*;

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {}

#[derive(Debug, Component)]
struct Position(f32);

#[derive(Debug, Component)]
struct Velocity(f32);

#[derive(Debug, Component)]
struct Active;

#[derive(Debug, Resource)]
struct DeltaTime(f32);

#[derive(Debug, Resource)]
struct Frame(u32);

fn integrate(
    mut bodies: Query<(&mut Position, &Velocity), With<Active>>,
    dt: Res<DeltaTime>,
    mut frame: ResMut<Frame>,
) {
    for (position, velocity) in bodies.iter() {
        position.0 += velocity.0 * dt.0;
    }
    frame.0 += 1;
}

fn main() {
    let mut world = World::new();
    world.spawn((Position(1.0), Velocity(4.0), Active)).unwrap();
    world.insert_resource(DeltaTime(0.5));
    world.insert_resource(Frame(0));

    let mut runtime = Runtime::new();
    // Ordinary registration is the proven-transferable path. No mobility
    // annotation is needed for a normal system whose parameters satisfy it.
    runtime.add_systems(Update, integrate).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let position = world.query::<&Position>().single(&world).unwrap();
    assert_eq!(position.0, 3.0);
    assert_eq!(world.resource::<Frame>().unwrap().0, 1);

    println!("frame 1: position={:.1}", position.0);
}
