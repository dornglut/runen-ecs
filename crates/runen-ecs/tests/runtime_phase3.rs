use runen_ecs::LocalCommands;
use runen_ecs::prelude::*;
use runen_ecs::{DeferredRecorderClass, QueryAccess, RuntimeError, SystemParam, SystemParamError};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Copy, Clone)]
struct GameplaySet;

impl SystemSet for GameplaySet {
    fn name(&self) -> &'static str {
        "GameplaySet"
    }
}

#[derive(Copy, Clone)]
struct PostGameplaySet;

impl SystemSet for PostGameplaySet {
    fn name(&self) -> &'static str {
        "PostGameplaySet"
    }
}

#[derive(Copy, Clone)]
struct LateObserveSet;

impl SystemSet for LateObserveSet {
    fn name(&self) -> &'static str {
        "LateObserveSet"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Marker(u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Extra(i32);

#[derive(Debug, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct IndexedName(String);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct SeenCount(u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct TargetEntity(Entity);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Step(u32);

struct LifetimeMarkerParam<'a>(PhantomData<&'a ()>);

unsafe impl<'a> SystemParam for LifetimeMarkerParam<'a> {
    type State = ();
    type Item<'world, 'state> = LifetimeMarkerParam<'world>;

    fn init_state() -> Result<Self::State, SystemParamError> {
        Ok(())
    }

    fn access(_state: &Self::State) -> QueryAccess {
        QueryAccess::default()
    }

    unsafe fn extract<'world, 'state>(
        _state: &'state mut Self::State,
        _context: runen_ecs::SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(LifetimeMarkerParam(PhantomData))
    }
}

#[derive(runen_ecs::SystemParam)]
struct LifetimeCollisionParamGroup<'w> {
    marker: LifetimeMarkerParam<'w>,
    step: Res<'w, Step>,
    seen: ResMut<'w, SeenCount>,
}

#[derive(runen_ecs::SystemParam)]
#[allow(dead_code)]
struct DerivedLocal<'w> {
    commands: LocalCommands<'w>,
}

#[derive(runen_ecs::SystemParam)]
#[allow(dead_code)]
struct NestedDerivedLocal<'w> {
    inner: DerivedLocal<'w>,
}

struct StructuralPretender;

unsafe impl SystemParam for StructuralPretender {
    type State = ();
    type Item<'world, 'state> = StructuralPretender;

    fn init_state() -> Result<Self::State, SystemParamError> {
        Ok(())
    }

    fn access(_: &Self::State) -> QueryAccess {
        QueryAccess::structural_mutation()
    }

    unsafe fn extract<'world, 'state>(
        _: &'state mut Self::State,
        _context: runen_ecs::SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        Ok(StructuralPretender)
    }
}

#[derive(runen_ecs::SystemParam)]
struct ConflictingResourceParamGroup<'w> {
    read: Res<'w, Step>,
    write: ResMut<'w, Step>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct SpawnGate(bool);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Resource)]
struct EmitCommands(bool);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct CountHistory(Vec<usize>);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct AddedChangedHistory(Vec<(usize, usize)>);

fn run_order_log() -> &'static Mutex<Vec<&'static str>> {
    static LOG: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Vec::new()))
}

fn push_run_order(label: &'static str) {
    run_order_log().lock().unwrap().push(label);
}

fn clear_run_order() {
    run_order_log().lock().unwrap().clear();
}

fn snapshot_run_order() -> Vec<&'static str> {
    run_order_log().lock().unwrap().clone()
}

#[test]
fn runtime_honors_in_set_before_and_after_ordering() {
    fn run_before_set() {
        push_run_order("before");
    }

    fn run_in_set() {
        push_run_order("in_set");
    }

    fn run_after_set() {
        push_run_order("after");
    }

    clear_run_order();
    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, run_in_set.in_set(GameplaySet));
    let _ = runtime.add_systems(Update, run_before_set.before(GameplaySet));
    let _ = runtime.add_systems(Update, run_after_set.after(GameplaySet));

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(snapshot_run_order(), vec!["before", "in_set", "after"]);
}

#[test]
fn semantic_ordering_cycle_is_rejected_deterministically() {
    fn first() {}
    fn second() {}

    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, first.in_set(GameplaySet).after(PostGameplaySet));
    let _ = runtime.add_systems(Update, second.in_set(PostGameplaySet).after(GameplaySet));

    let error = runtime
        .run_schedule::<Update>(&mut world)
        .expect_err("cyclic set ordering must be rejected");
    assert!(format!("{error:#}").contains("cyclic system ordering"));
}

#[test]
fn derive_handles_user_lifetime_named_w() {
    fn use_lifetime_group(mut group: LifetimeCollisionParamGroup<'_>) {
        let _ = &group.marker;
        group.seen.0 = group.step.0.saturating_add(1);
    }

    let mut world = World::new();
    world.insert_resource(Step(41));
    world.insert_resource(SeenCount(0));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, use_lifetime_group.on_invoker_thread());
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 42);
}

#[test]
fn grouped_conflicting_resource_borrows_are_rejected() {
    fn invalid_group(group: ConflictingResourceParamGroup<'_>) {
        let _ = (&group.read, &group.write);
    }

    let mut world = World::new();
    world.insert_resource(Step(0));
    let mut runtime = Runtime::new();
    let err = runtime
        .add_systems(Update, invalid_group)
        .err()
        .expect("conflicting group registration must fail");

    let message = format!("{err:#}");
    assert!(message.contains("conflicting param borrows"), "{message}");
}

#[test]
fn unordered_command_systems_merge_deterministically_without_visibility() {
    fn enqueue_first(mut commands: LocalCommands) {
        commands.spawn(Marker(1));
    }

    fn enqueue_second(mut commands: LocalCommands) {
        commands.spawn(Marker(2));
    }

    fn observe_unpublished_visibility(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(99));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        (
            enqueue_first.on_invoker_thread(),
            enqueue_second.on_invoker_thread(),
            observe_unpublished_visibility,
        ),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 2]);
}

#[test]
fn deferred_commands_publish_before_semantic_frontier_callback() {
    fn enqueue_producer(mut commands: LocalCommands) {
        commands.spawn(Marker(7));
    }

    fn observe_successor(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(0));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        enqueue_producer.on_invoker_thread().in_set(GameplaySet),
    );
    let _ = runtime.add_systems(
        Update,
        observe_successor.in_set(PostGameplaySet).after(GameplaySet),
    );

    let mut boundaries = Vec::new();
    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |frontier, world| {
                let marker_count = world.query_state::<&Marker, ()>().iter(&*world).count();
                boundaries.push((frontier.schedule().name(), frontier.ordinal(), marker_count));
                Ok::<(), std::io::Error>(())
            },
        )
        .unwrap();

    assert_eq!(boundaries, vec![("Update", 0, 1)]);
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 1);
}

#[test]
fn deferred_recorder_class_is_composed_structurally() {
    assert_eq!(
        <LocalCommands<'static> as SystemParam>::deferred_recorder_class().unwrap(),
        DeferredRecorderClass::LocalDeferred
    );
    assert_eq!(
        <DerivedLocal<'static> as SystemParam>::deferred_recorder_class().unwrap(),
        DeferredRecorderClass::LocalDeferred
    );
    assert_eq!(
        <NestedDerivedLocal<'static> as SystemParam>::deferred_recorder_class().unwrap(),
        DeferredRecorderClass::LocalDeferred
    );
    assert_eq!(
        <(DerivedLocal<'static>, LocalCommands<'static>) as SystemParam>::deferred_recorder_class()
            .unwrap(),
        DeferredRecorderClass::LocalDeferred
    );
    assert_eq!(
        <(
            (LocalCommands<'static>, Res<'static, SeenCount>),
            DerivedLocal<'static>
        ) as SystemParam>::deferred_recorder_class()
        .unwrap(),
        DeferredRecorderClass::LocalDeferred
    );
    assert_eq!(
        <(LocalCommands<'static>, LocalCommands<'static>) as SystemParam>::deferred_recorder_class(
        )
        .unwrap(),
        DeferredRecorderClass::LocalDeferred
    );
    assert_eq!(
        <Res<'static, SeenCount> as SystemParam>::deferred_recorder_class().unwrap(),
        DeferredRecorderClass::None
    );
}

#[test]
fn structural_access_does_not_infer_deferred_production() {
    fn structural_only(_: StructuralPretender) {}

    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, structural_only.on_invoker_thread());

    let mut frontiers = Vec::new();
    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |frontier, _| {
                frontiers.push(frontier.ordinal());
                Ok::<(), RuntimeError>(())
            },
        )
        .unwrap();

    assert!(frontiers.is_empty());
}

#[test]
fn schedule_without_deferred_parameters_has_no_frontier_callback() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, || {});

    let mut frontiers = Vec::new();
    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |frontier, _| {
                frontiers.push(frontier.ordinal());
                Ok::<(), RuntimeError>(())
            },
        )
        .unwrap();

    assert!(frontiers.is_empty());
}

#[test]
fn empty_deferred_buffer_still_reaches_its_structural_frontier() {
    fn empty_commands(_commands: LocalCommands) {}

    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, empty_commands.on_invoker_thread());

    let mut frontiers = Vec::new();
    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |frontier, _| {
                frontiers.push(frontier.ordinal());
                Ok::<(), RuntimeError>(())
            },
        )
        .unwrap();

    assert_eq!(frontiers, vec![0]);
}

#[test]
fn queue_empty_and_nonempty_runs_share_the_same_frontier_sequence() {
    fn conditional_producer(emit: Res<EmitCommands>, mut commands: LocalCommands) {
        if emit.0 {
            commands.spawn(Marker(7));
        }
    }
    fn successor() {}

    let mut world = World::new();
    world.insert_resource(EmitCommands(true));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        conditional_producer.on_invoker_thread().in_set(GameplaySet),
    );
    let _ = runtime.add_systems(Update, successor.after(GameplaySet));

    let mut nonempty_frontiers = Vec::new();
    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |frontier, _| {
                nonempty_frontiers.push(frontier.ordinal());
                Ok::<(), RuntimeError>(())
            },
        )
        .unwrap();

    world.resource_mut::<EmitCommands>().unwrap().0 = false;
    let mut empty_frontiers = Vec::new();
    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |frontier, _| {
                empty_frontiers.push(frontier.ordinal());
                Ok::<(), RuntimeError>(())
            },
        )
        .unwrap();

    assert_eq!(nonempty_frontiers, vec![0]);
    assert_eq!(empty_frontiers, nonempty_frontiers);
}

#[test]
fn frontier_callback_error_keeps_publication_and_stops_later_systems() {
    fn producer(mut commands: LocalCommands) {
        commands.spawn(Marker(11));
    }
    fn later(mut seen: ResMut<SeenCount>) {
        seen.0 = 1;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(0));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, producer.on_invoker_thread().in_set(GameplaySet));
    let _ = runtime.add_systems(Update, later.after(GameplaySet));

    let result = runtime.run_schedule_with_deferred_publication_frontier::<Update, _, _>(
        &mut world,
        |_frontier, world| {
            assert_eq!(world.query_state::<&Marker, ()>().iter(world).count(), 1);
            Err::<(), _>(std::io::Error::other("intentional frontier callback error"))
        },
    );

    assert!(matches!(result, Err(RuntimeError::Boundary { .. })));
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 1);
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
}

#[test]
fn frontier_callback_panic_preserves_publication_and_runtime_reuse() {
    fn producer(mut commands: LocalCommands) {
        commands.spawn(Marker(12));
    }
    fn later(mut seen: ResMut<SeenCount>) {
        seen.0 = 1;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(0));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, producer.on_invoker_thread().in_set(GameplaySet));
    let _ = runtime.add_systems(Update, later.after(GameplaySet));

    let first = catch_unwind(AssertUnwindSafe(|| {
        runtime.run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |_frontier, _world| -> Result<(), std::io::Error> {
                panic!("intentional frontier callback panic");
            },
        )
    }));
    assert!(first.is_err());
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 1);
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 2);
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 1);
}

#[test]
fn system_error_before_unreached_frontier_skips_callback_and_later_systems() {
    fn fail() -> Result<(), std::io::Error> {
        Err(std::io::Error::other("intentional system error"))
    }
    fn producer(mut commands: LocalCommands) {
        commands.spawn(Marker(18));
    }
    fn later(mut seen: ResMut<SeenCount>) {
        seen.0 = 1;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(0));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, fail.in_set(GameplaySet));
    let _ = runtime.add_systems(
        Update,
        producer
            .on_invoker_thread()
            .in_set(PostGameplaySet)
            .after(GameplaySet),
    );
    let _ = runtime.add_systems(Update, later.after(PostGameplaySet));

    let mut callbacks = 0;
    let result = runtime.run_schedule_with_deferred_publication_frontier::<Update, _, _>(
        &mut world,
        |_frontier, _world| {
            callbacks += 1;
            Ok::<(), RuntimeError>(())
        },
    );

    assert!(matches!(result, Err(RuntimeError::System { .. })));
    assert_eq!(callbacks, 0);
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 0);
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
}

#[test]
fn deferred_application_error_is_fail_stop_after_prior_commands() {
    fn producer(target: Res<TargetEntity>, mut commands: LocalCommands) {
        let entity = target.0;
        commands.queue(move |world| {
            world.despawn(entity)?;
            Ok(())
        });
        commands.queue(move |world| {
            world.despawn(entity)?;
            Ok(())
        });
        commands.spawn(Marker(13));
    }
    fn later(mut seen: ResMut<SeenCount>) {
        seen.0 = 1;
    }

    let mut world = World::new();
    let target = world.spawn(Marker(0)).unwrap();
    world.insert_resource(TargetEntity(target));
    world.insert_resource(SeenCount(0));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, producer.on_invoker_thread().in_set(GameplaySet));
    let _ = runtime.add_systems(Update, later.after(GameplaySet));

    let mut callbacks = 0;
    let result = runtime.run_schedule_with_deferred_publication_frontier::<Update, _, _>(
        &mut world,
        |_frontier, _world| {
            callbacks += 1;
            Ok::<(), RuntimeError>(())
        },
    );

    assert!(matches!(result, Err(RuntimeError::Command(_))));
    assert_eq!(callbacks, 0);
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 0);
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
}

#[test]
fn deferred_application_panic_is_fail_stop_without_leaking_later_commands() {
    fn producer(mut gate: ResMut<SpawnGate>, mut commands: LocalCommands) {
        commands.spawn(Marker(14));
        if !gate.0 {
            gate.0 = true;
            commands.queue(|_| -> Result<(), runen_ecs::CommandError> {
                panic!("intentional deferred application panic");
            });
            commands.spawn(Marker(15));
        } else {
            commands.spawn(Marker(16));
        }
    }

    let mut world = World::new();
    world.insert_resource(SpawnGate(false));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, producer.on_invoker_thread());

    let first = catch_unwind(AssertUnwindSafe(|| {
        runtime.run_schedule::<Update>(&mut world)
    }));
    assert!(first.is_err());
    let first_values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(first_values, vec![14]);

    runtime.run_schedule::<Update>(&mut world).unwrap();
    let mut second_values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    second_values.sort_unstable();
    assert_eq!(second_values, vec![14, 14, 16]);
}

#[test]
fn frontier_callback_runs_on_invoking_thread_with_exclusive_world() {
    fn producer(mut commands: LocalCommands) {
        commands.spawn(Marker(17));
    }

    let invoking_thread = std::thread::current().id();
    let mut world = World::new();
    world.insert_resource(SeenCount(0));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, producer.on_invoker_thread());

    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |_frontier, world| {
                assert_eq!(std::thread::current().id(), invoking_thread);
                world.resource_mut::<SeenCount>().unwrap().0 = 1;
                Ok::<(), RuntimeError>(())
            },
        )
        .unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 1);
}

#[test]
fn canonical_frontiers_are_multiple_and_later_frontier_may_be_empty() {
    fn first_producer(mut commands: LocalCommands) {
        commands.spawn(Marker(1));
    }
    fn second_producer(_commands: LocalCommands) {}
    fn prepare() {}
    fn first_successor(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }
    fn second_successor() {}

    let mut world = World::new();
    world.insert_resource(SeenCount(0));
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        first_producer.on_invoker_thread().in_set(GameplaySet),
    );
    let _ = runtime.add_systems(Update, first_successor.after(GameplaySet));
    let _ = runtime.add_systems(Update, prepare.in_set(LateObserveSet));
    let _ = runtime.add_systems(
        Update,
        second_producer
            .on_invoker_thread()
            .in_set(PostGameplaySet)
            .after(LateObserveSet),
    );
    let _ = runtime.add_systems(Update, second_successor.after(PostGameplaySet));

    let mut frontiers = Vec::new();
    runtime
        .run_schedule_with_deferred_publication_frontier::<Update, _, _>(
            &mut world,
            |frontier, world| {
                frontiers.push((
                    frontier.ordinal(),
                    world.query_state::<&Marker, ()>().iter(world).count(),
                ));
                Ok::<(), RuntimeError>(())
            },
        )
        .unwrap();

    assert_eq!(frontiers, vec![(0, 1), (1, 1)]);
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 1);
}

#[test]
fn closure_commands_queue_api_remains_functional() {
    let mut world = World::new();
    let mut commands = world.local_commands();
    commands.queue(|world| {
        let _ = world.spawn(Marker(33))?;
        Ok(())
    });

    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 0);
    commands.apply(&mut world).unwrap();

    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![33]);
}

#[test]
fn typed_deferred_commands_apply_correctly() {
    let mut world = World::new();
    let mut commands = world.local_commands();
    commands.queue(|world| {
        let _ = world.spawn(Marker(77))?;
        Ok(())
    });

    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 0);
    commands.apply(&mut world).unwrap();

    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![77]);
}

#[test]
fn mixed_closure_and_typed_commands_apply_in_deterministic_order() {
    let mut world = World::new();
    let mut commands = world.local_commands();
    commands.spawn(Marker(1));
    commands.queue(|world| {
        let _ = world.spawn(Marker(2))?;
        Ok(())
    });
    commands.queue(|world| {
        let _ = world.spawn(Marker(3))?;
        Ok(())
    });
    commands.queue(|world| {
        let _ = world.spawn(Marker(4))?;
        Ok(())
    });

    commands.apply(&mut world).unwrap();

    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 2, 3, 4]);
}

#[test]
fn batch_commands_apply_in_deterministic_insertion_order() {
    let mut world = World::new();
    let mut commands = world.local_commands();
    commands.batch(|batch| {
        batch.spawn(Marker(1));
        batch.queue(|world| {
            let _ = world.spawn(Marker(2))?;
            Ok(())
        });
        batch.queue(|world| {
            let _ = world.spawn(Marker(3))?;
            Ok(())
        });
    });

    commands.apply(&mut world).unwrap();

    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 2, 3]);
}

#[test]
fn batch_commands_do_not_mutate_before_publication() {
    fn enqueue_batch(mut commands: LocalCommands) {
        commands.batch(|batch| {
            batch.spawn(Marker(9));
        });
    }

    fn observe_unpublished_state(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(99));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        (enqueue_batch.on_invoker_thread(), observe_unpublished_state),
    );
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 1);
}

#[test]
fn batch_and_non_batch_commands_share_queue_order_deterministically() {
    let mut world = World::new();
    let mut commands = world.local_commands();
    commands.spawn(Marker(1));
    commands.batch(|batch| {
        batch.spawn(Marker(2));
    });
    commands.queue(|world| {
        let _ = world.spawn(Marker(3))?;
        Ok(())
    });
    commands.batch(|batch| {
        batch.queue(|world| {
            let _ = world.spawn(Marker(4))?;
            Ok(())
        });
    });

    commands.apply(&mut world).unwrap();

    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 2, 3, 4]);
}

#[test]
fn batch_supports_mixed_command_kinds() {
    let mut world = World::new();
    let entity = world.spawn(Marker(1)).expect("spawn should succeed");
    let mut commands = world.local_commands();
    commands.batch(|batch| {
        batch.queue(move |world| {
            world.insert(entity, Extra(5))?;
            Ok(())
        });
        batch.queue(move |world| {
            world.insert(entity, Extra(6))?;
            Ok(())
        });
        batch.remove::<Extra>(entity);
    });

    commands.apply(&mut world).unwrap();
    assert!(world.get::<Extra>(entity).is_none());
}

#[test]
fn batch_stops_on_first_error_and_keeps_earlier_mutations() {
    let mut world = World::new();
    let target = world.spawn(Marker(0)).expect("spawn should succeed");
    let mut commands = world.local_commands();
    commands.batch(|batch| {
        batch.spawn(Marker(10));
        batch.remove::<Extra>(target);
        batch.spawn(Marker(11));
    });

    let result = commands.apply(&mut world);
    assert!(matches!(
        result,
        Err(runen_ecs::CommandError::Entity(
            runen_ecs::EntityError::MissingComponent { .. }
        ))
    ));

    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![0, 10]);
}

#[test]
fn multiple_batches_in_one_schedule_keep_deterministic_system_order() {
    fn enqueue_batch_a(mut commands: LocalCommands) {
        commands.batch(|batch| {
            batch.spawn(Marker(1));
            batch.spawn(Marker(2));
        });
    }

    fn enqueue_batch_b(mut commands: LocalCommands) {
        commands.batch(|batch| {
            batch.spawn(Marker(3));
        });
    }

    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        (
            enqueue_batch_a.on_invoker_thread(),
            enqueue_batch_b.on_invoker_thread(),
        ),
    );
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 2, 3]);
}

#[test]
fn typed_commands_do_not_mutate_before_publication() {
    fn enqueue_typed(mut commands: LocalCommands) {
        commands.queue(|world| {
            let _ = world.spawn(Marker(9))?;
            Ok(())
        });
    }

    fn observe_unpublished_state(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(99));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        (enqueue_typed.on_invoker_thread(), observe_unpublished_state),
    );
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 1);
}

#[test]
fn typed_commands_follow_semantic_frontier_visibility_contract() {
    fn enqueue_typed_producer(target: Res<TargetEntity>, mut commands: LocalCommands) {
        let entity = target.0;
        commands.queue(move |world| {
            world.insert(entity, Extra(17))?;
            Ok(())
        });
    }

    fn observe_successor(mut seen: ResMut<SeenCount>, mut query: Query<&Extra>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    let target = world.spawn(Marker(1)).expect("spawn should succeed");
    world.insert_resource(TargetEntity(target));
    world.insert_resource(SeenCount(0));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        enqueue_typed_producer
            .on_invoker_thread()
            .in_set(GameplaySet),
    );
    let _ = runtime.add_systems(
        Update,
        observe_successor.in_set(PostGameplaySet).after(GameplaySet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 1);
    assert_eq!(world.require::<Extra>(target).unwrap().0, 17);
}

static NEXT_MARKER_ID: AtomicU32 = AtomicU32::new(0);

#[test]
fn borrowed_command_owner_is_stable_across_repeated_runs() {
    fn enqueue_a(mut commands: LocalCommands) {
        let id = NEXT_MARKER_ID.fetch_add(1, Ordering::SeqCst);
        commands.spawn(Marker(id));
    }

    fn enqueue_b(mut commands: LocalCommands) {
        let id = NEXT_MARKER_ID.fetch_add(1, Ordering::SeqCst);
        commands.spawn(Marker(id));
    }

    NEXT_MARKER_ID.store(0, Ordering::SeqCst);

    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        (enqueue_a.on_invoker_thread(), enqueue_b.on_invoker_thread()),
    );

    for _ in 0..20 {
        runtime.run_schedule::<Update>(&mut world).unwrap();
    }

    let mut ids = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(ids.len(), 40);
    assert_eq!(ids, (0..40).collect::<Vec<_>>());
}

#[test]
fn failed_schedule_drops_unpublished_deferred_commands_instead_of_replaying_next_run() {
    fn enqueue_then_fail_once(
        mut gate: ResMut<SpawnGate>,
        mut commands: LocalCommands,
    ) -> Result<(), std::io::Error> {
        if gate.0 {
            return Ok(());
        }
        commands.spawn(Marker(99));
        gate.0 = true;
        Err(std::io::Error::other("intentional failure"))
    }

    let mut world = World::new();
    world.insert_resource(SpawnGate(false));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, enqueue_then_fail_once.on_invoker_thread());

    let error = runtime.run_schedule::<Update>(&mut world).unwrap_err();
    assert!(matches!(error, RuntimeError::System { .. }));
    assert!(error.to_string().contains("intentional failure"));
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 0);

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 0);
}

static PARAM_INIT_CALLS: AtomicUsize = AtomicUsize::new(0);
static PARAM_EXTRACT_CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct StatefulParam(u32);

unsafe impl SystemParam for StatefulParam {
    type State = u32;
    type Item<'world, 'state> = StatefulParam;

    fn init_state() -> Result<Self::State, SystemParamError> {
        PARAM_INIT_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(0)
    }

    fn access(_state: &Self::State) -> QueryAccess {
        QueryAccess::default()
    }

    unsafe fn extract<'world, 'state>(
        state: &'state mut Self::State,
        _context: runen_ecs::SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        *state = state.saturating_add(1);
        PARAM_EXTRACT_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(StatefulParam(*state))
    }
}

#[test]
fn cached_system_param_state_reuse_is_stable_over_many_runs() {
    fn accumulate_state(counter: StatefulParam, mut seen: ResMut<SeenCount>) {
        seen.0 = seen.0.saturating_add(counter.0);
    }

    let init_before = PARAM_INIT_CALLS.load(Ordering::SeqCst);
    let extract_before = PARAM_EXTRACT_CALLS.load(Ordering::SeqCst);

    let mut world = World::new();
    world.insert_resource(SeenCount(0));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, accumulate_state.on_invoker_thread());

    for _ in 0..5 {
        runtime.run_schedule::<Update>(&mut world).unwrap();
    }

    assert_eq!(PARAM_INIT_CALLS.load(Ordering::SeqCst) - init_before, 1);
    assert_eq!(
        PARAM_EXTRACT_CALLS.load(Ordering::SeqCst) - extract_before,
        5
    );
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 15);
}

#[test]
fn publication_structural_migration_is_visible_in_semantic_successor() {
    fn queue_migration(
        mut step: ResMut<Step>,
        target: Res<TargetEntity>,
        mut commands: LocalCommands,
    ) {
        match step.0 {
            0 => commands.insert(target.0, Extra(7)),
            1 => commands.remove::<Extra>(target.0),
            2 => commands.insert(target.0, Extra(11)),
            _ => {}
        }
        step.0 = step.0.saturating_add(1);
    }

    fn observe_marker_extra(
        mut history: ResMut<CountHistory>,
        mut query: Query<(&Marker, &Extra)>,
    ) {
        history.0.push(query.iter().count());
    }

    let mut world = World::new();
    let target = world.spawn(Marker(1)).expect("spawn should succeed");
    world.insert_resource(TargetEntity(target));
    world.insert_resource(Step(0));
    world.insert_resource(CountHistory(Vec::new()));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        queue_migration.on_invoker_thread().in_set(GameplaySet),
    );
    let _ = runtime.add_systems(
        Update,
        observe_marker_extra
            .in_set(PostGameplaySet)
            .after(GameplaySet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<CountHistory>().unwrap().0, vec![1, 0, 1]);
    assert_eq!(world.require::<Extra>(target).unwrap().0, 11);
}

#[test]
fn system_order_controls_added_and_changed_visibility() {
    fn queue_spawn_once(mut gate: ResMut<SpawnGate>, mut commands: LocalCommands) {
        if gate.0 {
            return;
        }
        commands.spawn(Marker(5));
        gate.0 = true;
    }

    fn mutate_markers(mut query: Query<&mut Marker>) {
        for marker in query.iter() {
            marker.0 = marker.0.saturating_add(1);
        }
    }

    fn observe_added_changed(
        mut added: Query<&Marker, Added<Marker>>,
        mut changed: Query<&Marker, Changed<Marker>>,
        mut history: ResMut<AddedChangedHistory>,
    ) {
        history
            .0
            .push((added.iter().count(), changed.iter().count()));
    }

    let mut world = World::new();
    world.insert_resource(SpawnGate(false));
    world.insert_resource(AddedChangedHistory(Vec::new()));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        queue_spawn_once.on_invoker_thread().in_set(GameplaySet),
    );
    let _ = runtime.add_systems(
        Update,
        mutate_markers.in_set(PostGameplaySet).after(GameplaySet),
    );
    let _ = runtime.add_systems(
        Update,
        observe_added_changed
            .in_set(LateObserveSet)
            .after(PostGameplaySet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(
        world.resource::<AddedChangedHistory>().unwrap().0,
        vec![(1, 1), (0, 1)]
    );
}

#[test]
fn deferred_commands_keep_secondary_indexes_correct_after_apply() {
    fn queue_index_updates(
        mut step: ResMut<Step>,
        target: Res<TargetEntity>,
        mut commands: LocalCommands,
    ) {
        match step.0 {
            0 => commands.insert(target.0, IndexedName("renamed".to_string())),
            1 => commands.remove::<IndexedName>(target.0),
            2 => commands.insert(target.0, IndexedName("restored".to_string())),
            _ => {}
        }
        step.0 = step.0.saturating_add(1);
    }

    let mut world = World::new();
    world.ensure_component_index::<IndexedName, String>(|name| name.0.clone());
    let target = world
        .spawn(IndexedName("initial".to_string()))
        .expect("spawn should succeed");
    let other = world
        .spawn(IndexedName("other".to_string()))
        .expect("spawn should succeed");
    world.insert_resource(TargetEntity(target));
    world.insert_resource(Step(0));

    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, queue_index_updates.on_invoker_thread());

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(
        world.find_entity_by_index::<IndexedName, String>(&"renamed".to_string()),
        Some(target)
    );
    assert_eq!(
        world.find_entity_by_index::<IndexedName, String>(&"initial".to_string()),
        None
    );
    assert_eq!(
        world.find_entity_by_index::<IndexedName, String>(&"other".to_string()),
        Some(other)
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(
        world.find_entity_by_index::<IndexedName, String>(&"renamed".to_string()),
        None
    );
    assert_eq!(
        world.find_entity_by_index::<IndexedName, String>(&"other".to_string()),
        Some(other)
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(
        world.find_entity_by_index::<IndexedName, String>(&"restored".to_string()),
        Some(target)
    );
}
