use runen_ecs::prelude::*;

#[derive(ScheduleLabel)]
struct Update;

#[derive(Copy, Clone)]
struct Mutate;
impl SystemSet for Mutate {}

#[derive(Copy, Clone)]
struct Observe;
impl SystemSet for Observe {}

#[derive(Debug, Component)]
struct Actor;

#[derive(Debug, Component)]
struct Health(i32);

#[derive(Debug, Resource)]
struct Target(Entity);

#[derive(Debug, Resource)]
struct Phase(u8);

#[derive(Debug, Default, Resource)]
struct Observations {
    added: Vec<usize>,
    changed: Vec<usize>,
    removed: Vec<usize>,
}

fn mutate(
    phase: Res<Phase>,
    target: Res<Target>,
    mut health: Query<&mut Health>,
    mut commands: Commands,
) {
    match phase.0 {
        0 => commands.insert(target.0, Health(100)),
        1 => {
            let value = health.get(target.0).expect("health should exist");
            value.0 -= 10;
        }
        2 => commands.remove::<Health>(target.0),
        _ => {}
    }
}

fn observe_added(mut health: Query<&Health, Added<Health>>, mut seen: ResMut<Observations>) {
    seen.added.push(health.iter().count());
}

fn observe_changed(mut health: Query<&Health, Changed<Health>>, mut seen: ResMut<Observations>) {
    seen.changed.push(health.iter().count());
}

fn observe_removed(mut health: RemovedQuery<Health>, mut seen: ResMut<Observations>) {
    seen.removed.push(health.iter().count());
}

fn advance_phase(mut phase: ResMut<Phase>) {
    phase.0 += 1;
}

fn main() {
    let mut world = World::new();
    let target = world.spawn(Actor).unwrap();
    world.insert_resource(Target(target));
    world.insert_resource(Phase(0));
    world.insert_resource(Observations::default());

    let mut runtime = Runtime::new();
    runtime
        .add_systems(
            Update,
            (
                mutate.in_set(Mutate),
                observe_added.in_set(Observe).after(Mutate),
                observe_changed.in_set(Observe).after(Mutate),
                observe_removed.in_set(Observe).after(Mutate),
                advance_phase.after(Observe),
            ),
        )
        .unwrap();

    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let seen = world.resource::<Observations>().unwrap();
    assert_eq!(seen.added, vec![1, 0, 0]);
    // Insertion establishes change metadata too, so Added and Changed can both
    // observe the first publication window. Changed is conservative mutation
    // observation, not value-difference detection.
    assert_eq!(seen.changed, vec![1, 1, 0]);
    assert_eq!(seen.removed, vec![0, 0, 1]);

    println!(
        "Added={:?}, Changed={:?}, Removed={:?}",
        seen.added, seen.changed, seen.removed
    );
}
