use runen_ecs::prelude::*;
use runen_ecs::{QueryAccess, RuntimeError, SystemParam, SystemParamError};
use std::marker::PhantomData;
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
    fn name() -> &'static str {
        "GameplaySet"
    }
}

#[derive(Copy, Clone)]
struct PostGameplaySet;

impl SystemSet for PostGameplaySet {
    fn name() -> &'static str {
        "PostGameplaySet"
    }
}

#[derive(Copy, Clone)]
struct LateObserveSet;

impl SystemSet for LateObserveSet {
    fn name() -> &'static str {
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

    fn init_state(_world: &mut World) -> Result<Self::State, SystemParamError> {
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
struct ConflictingResourceParamGroup<'w> {
    read: Res<'w, Step>,
    write: ResMut<'w, Step>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct SpawnGate(bool);

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
    runtime.add_systems::<Update, _, _>(&mut world, run_in_set.in_set(GameplaySet));
    runtime.add_systems::<Update, _, _>(&mut world, run_before_set.before(GameplaySet));
    runtime.add_systems::<Update, _, _>(&mut world, run_after_set.after(GameplaySet));

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(snapshot_run_order(), vec!["before", "in_set", "after"]);
}

#[test]
fn semantic_ordering_cycle_is_rejected_deterministically() {
    fn first() {}
    fn second() {}

    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime
        .add_systems::<Update, _, _>(&mut world, first.in_set(GameplaySet).after(PostGameplaySet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        second.in_set(PostGameplaySet).after(GameplaySet),
    );

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
    runtime.add_systems::<Update, _, _>(&mut world, use_lifetime_group);
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
    runtime.add_systems::<Update, _, _>(&mut world, invalid_group);

    let err = runtime
        .run_schedule::<Update>(&mut world)
        .expect_err("read/write group for one resource should fail registration");
    let message = format!("{err:#}");
    assert!(message.contains("conflicting param borrows"), "{message}");
}

#[test]
fn structural_command_systems_share_stage_and_merge_deterministically() {
    fn enqueue_first(mut commands: Commands) {
        commands.spawn(Marker(1));
    }

    fn enqueue_second(mut commands: Commands) {
        commands.spawn(Marker(2));
    }

    fn observe_stage_visibility(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(99));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (enqueue_first, enqueue_second, observe_stage_visibility),
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
fn deferred_commands_flush_before_ecs_deferred_apply_boundary_callback() {
    fn enqueue_stage(mut commands: Commands) {
        commands.spawn(Marker(7));
    }

    fn observe_followup_stage(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(0));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, enqueue_stage.in_set(GameplaySet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_followup_stage
            .in_set(PostGameplaySet)
            .after(GameplaySet),
    );

    let mut boundaries = Vec::new();
    runtime
        .run_schedule_with_deferred_apply_boundary::<Update, _, _>(&mut world, |boundary, world| {
            let marker_count = world.query_state::<&Marker, ()>().iter(&*world).count();
            boundaries.push((boundary.schedule().name(), boundary.index(), marker_count));
            Ok::<(), std::io::Error>(())
        })
        .unwrap();

    assert_eq!(boundaries, vec![("Update", 0, 1), ("Update", 1, 1)]);
    assert_eq!(world.resource::<SeenCount>().unwrap().0, 1);
}

#[test]
fn closure_commands_queue_api_remains_functional() {
    let mut world = World::new();
    let mut commands = world.commands();
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
    let mut commands = world.commands();
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
    let mut commands = world.commands();
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
    let mut commands = world.commands();
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
fn batch_commands_do_not_mutate_before_stage_flush() {
    fn enqueue_batch(mut commands: Commands) {
        commands.batch(|batch| {
            batch.spawn(Marker(9));
        });
    }

    fn observe_same_stage(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(99));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (enqueue_batch, observe_same_stage));
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 1);
}

#[test]
fn batch_and_non_batch_commands_share_queue_order_deterministically() {
    let mut world = World::new();
    let mut commands = world.commands();
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
    let mut commands = world.commands();
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
    let mut commands = world.commands();
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
fn multiple_batches_in_one_stage_keep_deterministic_system_order() {
    fn enqueue_batch_a(mut commands: Commands) {
        commands.batch(|batch| {
            batch.spawn(Marker(1));
            batch.spawn(Marker(2));
        });
    }

    fn enqueue_batch_b(mut commands: Commands) {
        commands.batch(|batch| {
            batch.spawn(Marker(3));
        });
    }

    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (enqueue_batch_a, enqueue_batch_b));
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let values = world
        .query_state::<&Marker, ()>()
        .iter(&world)
        .map(|marker| marker.0)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 2, 3]);
}

#[test]
fn typed_commands_do_not_mutate_before_stage_flush() {
    fn enqueue_typed(mut commands: Commands) {
        commands.queue(|world| {
            let _ = world.spawn(Marker(9))?;
            Ok(())
        });
    }

    fn observe_same_stage(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(99));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (enqueue_typed, observe_same_stage));
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 1);
}

#[test]
fn typed_commands_follow_stage_boundary_visibility_contract() {
    fn enqueue_stage_typed(target: Res<TargetEntity>, mut commands: Commands) {
        let entity = target.0;
        commands.queue(move |world| {
            world.insert(entity, Extra(17))?;
            Ok(())
        });
    }

    fn observe_followup_stage(mut seen: ResMut<SeenCount>, mut query: Query<&Extra>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    let target = world.spawn(Marker(1)).expect("spawn should succeed");
    world.insert_resource(TargetEntity(target));
    world.insert_resource(SeenCount(0));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, enqueue_stage_typed.in_set(GameplaySet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_followup_stage
            .in_set(PostGameplaySet)
            .after(GameplaySet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 1);
    assert_eq!(world.require::<Extra>(target).unwrap().0, 17);
}

static NEXT_MARKER_ID: AtomicU32 = AtomicU32::new(0);

#[test]
fn borrowed_command_owner_is_stable_across_repeated_runs() {
    fn enqueue_a(mut commands: Commands) {
        let id = NEXT_MARKER_ID.fetch_add(1, Ordering::SeqCst);
        commands.spawn(Marker(id));
    }

    fn enqueue_b(mut commands: Commands) {
        let id = NEXT_MARKER_ID.fetch_add(1, Ordering::SeqCst);
        commands.spawn(Marker(id));
    }

    NEXT_MARKER_ID.store(0, Ordering::SeqCst);

    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (enqueue_a, enqueue_b));

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
fn failed_schedule_drops_stage_deferred_commands_instead_of_replaying_next_run() {
    fn enqueue_then_fail_once(
        mut gate: ResMut<SpawnGate>,
        mut commands: Commands,
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
    runtime.add_systems::<Update, _, _>(&mut world, enqueue_then_fail_once);

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

    fn init_state(_world: &mut World) -> Result<Self::State, SystemParamError> {
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
    runtime.add_systems::<Update, _, _>(&mut world, accumulate_state);

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
fn flush_stage_structural_migration_is_visible_in_followup_stage() {
    fn queue_migration(mut step: ResMut<Step>, target: Res<TargetEntity>, mut commands: Commands) {
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
    runtime.add_systems::<Update, _, _>(&mut world, queue_migration.in_set(GameplaySet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
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
    fn queue_spawn_once(mut gate: ResMut<SpawnGate>, mut commands: Commands) {
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
    runtime.add_systems::<Update, _, _>(&mut world, queue_spawn_once.in_set(GameplaySet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        mutate_markers.in_set(PostGameplaySet).after(GameplaySet),
    );
    runtime.add_systems::<Update, _, _>(
        &mut world,
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
        mut commands: Commands,
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
    runtime.add_systems::<Update, _, _>(&mut world, queue_index_updates);

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
