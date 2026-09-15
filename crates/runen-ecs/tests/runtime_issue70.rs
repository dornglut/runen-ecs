use runen_ecs::prelude::*;
use runen_ecs::{Commands, LocalCommands, RemovedQuery, RuntimeError, ScheduleValidationError};

#[derive(Copy, Clone)]
struct Update;
impl ScheduleLabel for Update {}

#[derive(Copy, Clone)]
struct ProducerSet;
impl SystemSet for ProducerSet {}

#[derive(Copy, Clone)]
struct MissingSet;
impl SystemSet for MissingSet {}

#[derive(Copy, Clone)]
struct CycleA;
impl SystemSet for CycleA {}

#[derive(Copy, Clone)]
struct CycleB;
impl SystemSet for CycleB {}

#[derive(runen_ecs::Component)]
struct Marker;

#[derive(runen_ecs::Resource)]
struct Count(u32);

#[derive(runen_ecs::Resource)]
struct StartupValue(u32);

#[derive(Default, runen_ecs::Resource)]
struct ObservationHistory(Vec<(usize, usize, usize)>);

#[test]
fn registration_supports_transferable_and_invoker_thread_systems() {
    fn transferable(mut count: ResMut<Count>) {
        count.0 += 1;
    }

    fn invoker_thread(mut count: ResMut<Count>) {
        count.0 += 10;
    }

    let mut world = World::new();
    world.insert_resource(Count(0));
    let mut runtime = Runtime::new();
    runtime
        .add_systems(Update, (transferable, invoker_thread.on_invoker_thread()))
        .unwrap();

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.resource::<Count>().unwrap().0, 11);
}

#[test]
fn registration_rejects_invalid_borrows_and_deferred_recorder_mixes_immediately() {
    fn invalid_borrow(_: Res<Count>, _: ResMut<Count>) {}
    fn invalid_recorder(_: LocalCommands<'_>, _: Commands<'_>) {}

    let mut runtime = Runtime::new();
    assert!(runtime.add_systems(Update, invalid_borrow).err().is_some());
    assert!(
        runtime
            .add_systems(Update, invalid_recorder.on_invoker_thread())
            .err()
            .is_some()
    );
}

#[test]
fn tuple_registration_is_all_or_nothing() {
    fn would_run(mut count: ResMut<Count>) {
        count.0 += 1;
    }
    fn invalid(_: LocalCommands<'_>, _: Commands<'_>) {}
    fn later(mut count: ResMut<Count>) {
        count.0 += 10;
    }

    let mut world = World::new();
    world.insert_resource(Count(0));
    let mut runtime = Runtime::new();
    assert!(
        runtime
            .add_systems(
                Update,
                (would_run.on_invoker_thread(), invalid.on_invoker_thread()),
            )
            .err()
            .is_some()
    );
    runtime.add_systems(Update, later).unwrap();

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.resource::<Count>().unwrap().0, 10);
}

#[test]
fn missing_resource_is_allowed_at_registration_and_fails_with_context_at_invocation() {
    fn consume(_: Res<StartupValue>) {}

    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems(Update, consume).unwrap();
    let error = runtime.run_schedule::<Update>(&mut world).unwrap_err();
    match error {
        RuntimeError::Param { system, source } => {
            assert!(system.contains("consume"));
            assert!(matches!(source, runen_ecs::SystemParamError::Resource(_)));
        }
        other => panic!("expected structured parameter error, got {other:?}"),
    }
}

#[test]
fn startup_producer_is_visible_to_later_consumer_without_reregistration() {
    fn produce(mut commands: LocalCommands) {
        commands.queue(|world| {
            world.insert_resource(StartupValue(7));
            Ok(())
        });
    }
    fn consume(value: Res<StartupValue>, mut count: ResMut<Count>) {
        count.0 = value.0;
    }

    let mut world = World::new();
    world.insert_resource(Count(0));
    let mut runtime = Runtime::new();
    runtime
        .add_systems(
            Update,
            (
                produce.on_invoker_thread().in_set(ProducerSet),
                consume.after(ProducerSet),
            ),
        )
        .unwrap();

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.resource::<Count>().unwrap().0, 7);
}

#[test]
fn registered_queries_bind_lazily_and_reset_added_changed_removed_by_lineage() {
    fn observe(
        mut added: Query<&Marker, Added<Marker>>,
        mut changed: Query<&Marker, Changed<Marker>>,
        mut removed: RemovedQuery<Marker>,
        mut history: ResMut<ObservationHistory>,
    ) {
        history.0.push((
            added.iter().count(),
            changed.iter().count(),
            removed.iter().count(),
        ));
    }

    let mut first = World::new();
    let entity = first.spawn(Marker).unwrap();
    first.insert_resource(ObservationHistory::default());
    let mut runtime = Runtime::new();
    runtime.add_systems(Update, observe).unwrap();

    runtime.run_schedule::<Update>(&mut first).unwrap();
    first.despawn(entity).unwrap();
    runtime.run_schedule::<Update>(&mut first).unwrap();
    assert_eq!(
        first.resource::<ObservationHistory>().unwrap().0,
        vec![(1, 1, 0), (0, 0, 1)]
    );

    let mut second = World::new();
    second.spawn(Marker).unwrap();
    second.insert_resource(ObservationHistory::default());
    runtime.run_schedule::<Update>(&mut second).unwrap();
    assert_eq!(
        second.resource::<ObservationHistory>().unwrap().0,
        vec![(1, 1, 0)]
    );
}

#[test]
fn validate_and_dirty_run_or_inspect_rebuild_schedule_topology() {
    fn source() {}
    fn target() {}

    let mut runtime = Runtime::new();
    runtime
        .add_systems(Update, source.before(MissingSet))
        .unwrap();
    assert!(matches!(runtime.validate(), Err(RuntimeError::Schedule(_))));
    assert!(matches!(
        runtime.inspect_schedule::<Update>(),
        Err(RuntimeError::Schedule(_))
    ));

    let mut cyclic = Runtime::new();
    cyclic
        .add_systems(
            Update,
            (
                source.in_set(CycleA).after(CycleB),
                target.in_set(CycleB).after(CycleA),
            ),
        )
        .unwrap();
    assert!(matches!(
        cyclic.validate(),
        Err(RuntimeError::Schedule(
            ScheduleValidationError::OrderingCycle { .. }
        ))
    ));

    let mut valid = Runtime::new();
    valid.add_systems(Update, source).unwrap();
    assert!(valid.inspect_schedule::<Update>().unwrap().is_some());
    valid.add_systems(Update, target).unwrap();
    assert!(valid.inspect_schedule::<Update>().unwrap().is_some());
}
