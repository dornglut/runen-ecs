use runen_ecs::{
    BatchCommands, Commands, DeferredRecorderClass, LocalCommands, ResMut, Runtime,
    SystemMobilityExt, SystemParam, SystemParamContext, SystemParamError, World,
};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Copy, Clone)]
struct Update;

impl runen_ecs::ScheduleLabel for Update {
    fn name() -> &'static str {
        "CommandsUpdate"
    }
}

#[derive(Debug, PartialEq, Eq, runen_ecs::Resource)]
struct Events(Vec<&'static str>);

#[derive(Debug, PartialEq, Eq, runen_ecs::Resource)]
struct Gate(bool);

#[derive(runen_ecs::SystemParam)]
struct TransferableGroup<'w> {
    first: Commands<'w>,
    second: Commands<'w>,
}

#[derive(runen_ecs::SystemParam)]
#[allow(dead_code)]
struct NestedTransferableGroup<'w> {
    inner: TransferableGroup<'w>,
}

struct TransferProbe;

static TRANSFER_PROBE_INIT: AtomicUsize = AtomicUsize::new(0);

unsafe impl SystemParam for TransferProbe {
    type State = ();
    type Item<'world, 'state> = TransferProbe;

    fn init_state(_: &mut World) -> Result<Self::State, SystemParamError> {
        TRANSFER_PROBE_INIT.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn deferred_recorder_class()
    -> Result<DeferredRecorderClass, runen_ecs::DeferredRecorderConflict> {
        Ok(DeferredRecorderClass::TransferableDeferred)
    }

    fn access(_: &Self::State) -> runen_ecs::QueryAccess {
        runen_ecs::QueryAccess::default()
    }

    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        _: SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(TransferProbe)
    }
}

fn event(world: &mut World, label: &'static str) -> Result<(), runen_ecs::CommandError> {
    world.resource_mut::<Events>().unwrap().0.push(label);
    Ok(())
}

#[test]
fn recorder_classes_have_one_structured_fallible_composition() {
    assert_eq!(
        DeferredRecorderClass::None
            .merge(DeferredRecorderClass::TransferableDeferred)
            .unwrap(),
        DeferredRecorderClass::TransferableDeferred
    );
    assert_eq!(
        DeferredRecorderClass::TransferableDeferred
            .merge(DeferredRecorderClass::TransferableDeferred)
            .unwrap(),
        DeferredRecorderClass::TransferableDeferred
    );
    let conflict = DeferredRecorderClass::LocalDeferred
        .merge(DeferredRecorderClass::TransferableDeferred)
        .unwrap_err();
    assert_eq!(conflict.local(), DeferredRecorderClass::LocalDeferred);
    assert_eq!(
        conflict.transferable(),
        DeferredRecorderClass::TransferableDeferred
    );
    assert!(
        <(LocalCommands<'static>, Commands<'static>) as SystemParam>::deferred_recorder_class()
            .is_err()
    );
    assert!(<NestedTransferableGroup<'static> as SystemParam>::deferred_recorder_class().is_ok());
    assert_eq!(
        <NestedTransferableGroup<'static> as SystemParam>::deferred_recorder_class().unwrap(),
        DeferredRecorderClass::TransferableDeferred
    );
}

#[test]
fn same_class_handles_share_one_ordered_transferable_buffer() {
    fn record(mut group: TransferableGroup<'_>) {
        group.first.queue(|world: &mut World| event(world, "first"));
        group.second.batch(|batch: &mut BatchCommands| {
            batch.queue(|world: &mut World| event(world, "second"));
            batch.queue(|world: &mut World| event(world, "third"));
        });
    }

    let mut world = World::new();
    world.insert_resource(Events(Vec::new()));
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, record);

    let mut frontiers = Vec::new();
    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |frontier, world| {
                frontiers.push(frontier.ordinal());
                assert_eq!(
                    world.resource::<Events>().unwrap().0,
                    ["first", "second", "third"]
                );
                Ok::<(), std::convert::Infallible>(())
            },
        )
        .unwrap();

    assert_eq!(frontiers, [0]);
    assert_eq!(
        world.resource::<Events>().unwrap().0,
        ["first", "second", "third"]
    );
}

#[test]
fn transfer_buffer_is_discarded_on_error_and_panic() {
    fn failing(mut gate: ResMut<Gate>, mut commands: Commands<'_>) -> Result<(), std::io::Error> {
        if gate.0 {
            gate.0 = false;
            commands.queue(|world: &mut World| event(world, "error-leak"));
            return Err(std::io::Error::other("expected system error"));
        }
        commands.queue(|world: &mut World| event(world, "after-error"));
        Ok(())
    }

    let mut world = World::new();
    world.insert_resource(Events(Vec::new()));
    world.insert_resource(Gate(true));
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, failing);

    assert!(runtime.run_schedule::<Update>(&mut world).is_err());
    assert!(world.resource::<Events>().unwrap().0.is_empty());
    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.resource::<Events>().unwrap().0, ["after-error"]);

    fn panicking(mut gate: ResMut<Gate>, mut commands: Commands<'_>) {
        if gate.0 {
            gate.0 = false;
            commands.queue(|world: &mut World| event(world, "panic-leak"));
            panic!("expected system panic");
        }
        commands.queue(|world: &mut World| event(world, "after-panic"));
    }

    let mut world = World::new();
    world.insert_resource(Events(Vec::new()));
    world.insert_resource(Gate(true));
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, panicking);
    let result = catch_unwind(AssertUnwindSafe(|| {
        runtime.run_schedule::<Update>(&mut world)
    }));
    assert!(result.is_err());
    assert!(world.resource::<Events>().unwrap().0.is_empty());
    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.resource::<Events>().unwrap().0, ["after-panic"]);
}

#[test]
fn mixed_recorder_graph_is_rejected_before_state_initialization() {
    TRANSFER_PROBE_INIT.store(0, Ordering::Relaxed);
    fn mixed(_: LocalCommands<'_>, _: TransferProbe) {}

    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, mixed.on_invoker_thread());

    let error = runtime.run_schedule::<Update>(&mut world).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mixed deferred recorder capabilities")
    );
    assert_eq!(TRANSFER_PROBE_INIT.load(Ordering::Relaxed), 0);
}

#[test]
fn transfer_batch_is_send_after_erased_effects_are_recorded() {
    fn assert_send<T: Send>() {}
    assert_send::<BatchCommands>();
}

#[test]
fn world_entrypoints_make_transfer_safe_commands_the_default() {
    let mut world = World::new();
    world.insert_resource(Events(Vec::new()));

    let mut commands = world.commands();
    commands.queue(|world: &mut World| event(world, "default"));
    commands.apply(&mut world).unwrap();
    assert_eq!(world.resource::<Events>().unwrap().0, ["default"]);

    let local_ran = Rc::new(Cell::new(false));
    let captured = Rc::clone(&local_ran);
    let mut local_commands = world.local_commands();
    local_commands.queue(move |_world: &mut World| {
        captured.set(true);
        Ok(())
    });
    local_commands.apply(&mut world).unwrap();
    assert!(local_ran.get());
}

#[test]
fn local_commands_succeed_with_explicit_invoker_thread_registration() {
    let mut world = World::new();
    let local_ran = Rc::new(Cell::new(false));
    let captured = Rc::clone(&local_ran);
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (move |mut commands: LocalCommands<'_>| {
            let deferred_capture = Rc::clone(&captured);
            commands.queue(move |_world: &mut World| {
                deferred_capture.set(true);
                Ok(())
            });
        })
        .on_invoker_thread(),
    );
    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert!(local_ran.get());
}
