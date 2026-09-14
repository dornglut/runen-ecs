use runen_ecs::prelude::*;

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Debug, runen_ecs::Component)]
struct Position(i32);

#[derive(Debug, runen_ecs::Component)]
struct Ready;

#[derive(Debug, runen_ecs::Component)]
struct Spawned;

#[derive(Debug, runen_ecs::Resource)]
struct Target(Entity);

#[derive(Debug, Default, runen_ecs::Resource)]
struct Visibility {
    ready_before: usize,
    ready_after_queue: usize,
    spawned_before: usize,
    spawned_after_queue: usize,
}

fn stage_changes(
    target: Res<Target>,
    mut ready: Query<&Position, With<Ready>>,
    mut spawned: Query<&Position, With<Spawned>>,
    mut commands: Commands,
    mut visibility: ResMut<Visibility>,
) {
    visibility.ready_before = ready.iter().count();
    visibility.spawned_before = spawned.iter().count();

    commands.insert(target.0, Ready);
    commands.spawn((Position(99), Spawned));

    // Deferred commands are staged, not applied to the live World immediately.
    visibility.ready_after_queue = ready.iter().count();
    visibility.spawned_after_queue = spawned.iter().count();
}

fn main() {
    let mut world = World::new();
    let target = world.spawn(Position(1)).unwrap();
    world.insert_resource(Target(target));
    world.insert_resource(Visibility::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, stage_changes);
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let visibility = world.resource::<Visibility>().unwrap();
    assert_eq!(visibility.ready_before, 0);
    assert_eq!(visibility.ready_after_queue, 0);
    assert_eq!(visibility.spawned_before, 0);
    assert_eq!(visibility.spawned_after_queue, 0);

    let ready_after_publish = world
        .query_state::<&Position, With<Ready>>()
        .single(&world)
        .expect("the target should become ready after publication");
    let spawned_after_publish = world
        .query_state::<&Position, With<Spawned>>()
        .single(&world)
        .expect("the staged spawn should exist after publication");
    assert_eq!(ready_after_publish.0, 1);
    assert_eq!(spawned_after_publish.0, 99);

    println!("staged changes became visible after schedule publication");
}
