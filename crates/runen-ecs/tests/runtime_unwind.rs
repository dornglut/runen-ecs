use runen_ecs::LocalCommands;
use runen_ecs::prelude::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Copy, Clone)]
struct CommittedProducerSet;

impl SystemSet for CommittedProducerSet {
    fn name() -> &'static str {
        "CommittedProducerSet"
    }
}

#[derive(Copy, Clone)]
struct LaterFailureSet;

impl SystemSet for LaterFailureSet {
    fn name() -> &'static str {
        "LaterFailureSet"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component)]
struct Marker(u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Resource)]
struct PanicGate(bool);

fn marker_count(world: &mut World) -> usize {
    world.query_state::<&Marker, ()>().iter(&*world).count()
}

#[test]
fn panicked_schedule_discards_unpublished_deferred_work_before_runtime_reuse() {
    fn enqueue(mut commands: LocalCommands) {
        commands.spawn(Marker(1));
    }

    fn panic_once(mut gate: ResMut<PanicGate>) {
        if !gate.0 {
            gate.0 = true;
            panic!("intentional system panic");
        }
    }

    let mut world = World::new();
    world.insert_resource(PanicGate(false));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (enqueue.on_invoker_thread(), panic_once));

    let first_run = catch_unwind(AssertUnwindSafe(|| {
        runtime.run_schedule::<Update>(&mut world)
    }));
    assert!(
        first_run.is_err(),
        "the first run must propagate the system panic"
    );
    assert_eq!(marker_count(&mut world), 0);

    runtime
        .run_schedule::<Update>(&mut world)
        .expect("runtime must remain reusable after the caught panic");
    assert_eq!(marker_count(&mut world), 1);
}

#[test]
fn panicked_later_system_preserves_committed_frontier_and_discards_only_unpublished_work() {
    fn enqueue_committed(mut commands: LocalCommands) {
        commands.spawn(Marker(1));
    }

    fn enqueue_aborted(mut commands: LocalCommands) {
        commands.spawn(Marker(2));
    }

    fn panic_once(mut gate: ResMut<PanicGate>) {
        if !gate.0 {
            gate.0 = true;
            panic!("intentional later-system panic");
        }
    }

    let mut world = World::new();
    world.insert_resource(PanicGate(false));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        enqueue_committed
            .on_invoker_thread()
            .in_set(CommittedProducerSet),
    );
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (
            enqueue_aborted
                .on_invoker_thread()
                .in_set(LaterFailureSet)
                .after(CommittedProducerSet),
            panic_once
                .in_set(LaterFailureSet)
                .after(CommittedProducerSet),
        ),
    );

    let first_run = catch_unwind(AssertUnwindSafe(|| {
        runtime.run_schedule::<Update>(&mut world)
    }));
    assert!(
        first_run.is_err(),
        "the later system must propagate the system panic"
    );
    assert_eq!(marker_count(&mut world), 1);

    runtime
        .run_schedule::<Update>(&mut world)
        .expect("runtime must remain reusable after the caught panic");

    let mut values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    values.sort_unstable();
    assert_eq!(values, vec![1, 1, 2]);
}
