use runen_ecs::prelude::*;

#[derive(runen_ecs::Component, runen_ecs::Resource)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(runen_ecs::Component, runen_ecs::Resource)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(runen_ecs::Component, runen_ecs::Resource)]
struct Simulated;

#[derive(runen_ecs::Component, runen_ecs::Resource)]
struct DeltaTime(pub f32);

#[derive(runen_ecs::Component, runen_ecs::Resource)]
struct Frame(pub u64);

fn tick(
    mut query: Query<(&mut Position, &Velocity), With<Simulated>>,
    dt: Res<DeltaTime>,
    mut frame: ResMut<Frame>,
) {
    for (pos, vel) in query.iter() {
        pos.x += vel.x * dt.0;
        pos.y += vel.y * dt.0;
    }
    frame.0 += 1;
}

#[test]
fn docs_gameplay_signatures_compile() {
    let _ = tick as fn(
        Query<(&mut Position, &Velocity), With<Simulated>>,
        Res<DeltaTime>,
        ResMut<Frame>,
    );
}

#[test]
fn docs_runtime_query_snippet_runs() {
    let mut world = World::new();
    world
        .spawn((
            Position { x: 1.0, y: 2.0 },
            Velocity { x: 0.5, y: -1.0 },
            Simulated,
        ))
        .expect("spawn should succeed");
    world.insert_resource(DeltaTime(0.5));
    world.insert_resource(Frame(1));

    let query = world
        .query_state::<(&mut Position, &Velocity), ()>()
        .with::<Simulated>();
    for (position, velocity) in query.iter(&mut world) {
        position.x += velocity.x;
        position.y += velocity.y;
    }

    assert_eq!(world.resource::<Frame>().unwrap().0, 1);
}

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[test]
fn docs_runtime_execution_snippet_runs() {
    fn advance(mut query: Query<&mut Position>, dt: Res<DeltaTime>, mut frame: ResMut<Frame>) {
        for position in query.iter() {
            position.x += dt.0;
            position.y += dt.0;
        }
        frame.0 += 1;
    }

    let mut world = World::new();
    world
        .spawn(Position { x: 0.0, y: 0.0 })
        .expect("spawn should succeed");
    world.insert_resource(DeltaTime(0.25));
    world.insert_resource(Frame(0));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, advance);
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let pos = world.query_state::<&Position, ()>().single(&world).unwrap();
    assert_eq!(pos.x, 0.25);
    assert_eq!(world.resource::<Frame>().unwrap().0, 1);
}

#[test]
fn docs_change_filter_snippet_runs() {
    let mut world = World::new();
    let entity = world
        .spawn(Position { x: 0.0, y: 0.0 })
        .expect("spawn should succeed");
    let changed_positions = world.query_state::<(Entity, &Position), Changed<Position>>();

    let first: Vec<_> = changed_positions
        .iter(&world)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(first, vec![entity]);
    assert!(changed_positions.iter(&world).next().is_none());

    world.require_mut::<Position>(entity).unwrap().x = 1.0;
    let second: Vec<_> = changed_positions
        .iter(&world)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(second, vec![entity]);
}
