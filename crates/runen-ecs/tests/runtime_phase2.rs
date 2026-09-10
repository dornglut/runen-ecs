use runen_ecs::prelude::*;
use runen_ecs::{QueryAccess, SystemParam, SystemParamError};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct Position(f32);

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct Velocity(f32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Frame(u64);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Score(u64);

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct DeltaTime(f32);

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct Bonus(f32);

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Resource)]
struct Scale(f32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Resource)]
struct ExtraScore(u64);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Marker;

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct SeenCount(u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct MissingRes;

#[test]
fn runtime_executes_1_2_and_8_param_systems() {
    fn bump_frame(mut frame: ResMut<Frame>) {
        frame.0 += 1;
    }

    fn integrate_positions(mut query: Query<&mut Position>, dt: Res<DeltaTime>) {
        for position in query.iter() {
            position.0 += dt.0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn full_tick(
        mut query: Query<(&mut Position, Option<&Velocity>)>,
        dt: Res<DeltaTime>,
        mut frame: ResMut<Frame>,
        mut score: ResMut<Score>,
        mut commands: Commands,
        bonus: Res<Bonus>,
        scale: Res<Scale>,
        extra_score: Res<ExtraScore>,
    ) {
        for (position, velocity) in query.iter() {
            position.0 += dt.0 + bonus.0 + scale.0 + velocity.map_or(0.0, |v| v.0);
        }
        frame.0 += 10;
        score.0 += extra_score.0;
        commands.spawn(Marker);
    }

    let mut world = World::new();
    world
        .spawn((Position(1.0), Velocity(2.0)))
        .expect("spawn should succeed");
    world.spawn(Position(2.0)).expect("spawn should succeed");
    world.insert_resource(Frame(0));
    world.insert_resource(Score(0));
    world.insert_resource(DeltaTime(0.5));
    world.insert_resource(Bonus(1.0));
    world.insert_resource(Scale(0.25));
    world.insert_resource(ExtraScore(7));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (bump_frame, integrate_positions, full_tick));
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let positions: Vec<_> = world
        .query_state::<&Position, ()>()
        .iter(&world)
        .map(|position| position.0)
        .collect();
    assert_eq!(positions, vec![5.25, 4.25]);
    assert_eq!(world.resource::<Frame>().unwrap().0, 11);
    assert_eq!(world.resource::<Score>().unwrap().0, 7);
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 1);
}

static INIT_STATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static EXTRACT_CALLS: AtomicUsize = AtomicUsize::new(0);

struct CachedCounter(usize);

// Safety: this test-only parameter has no World access and only mutates its own
// per-system cached state. The empty QueryAccess therefore describes extraction
// completely.
unsafe impl SystemParam for CachedCounter {
    type State = usize;
    type Item<'world, 'state> = CachedCounter;

    fn init_state(_world: &mut World) -> Result<Self::State, SystemParamError> {
        INIT_STATE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(0)
    }

    fn access(_state: &Self::State) -> QueryAccess {
        QueryAccess::default()
    }

    unsafe fn extract<'world, 'state>(
        state: &'state mut Self::State,
        _context: runen_ecs::SystemParamContext<'world>,
    ) -> Result<Self::Item<'world, 'state>, SystemParamError> {
        *state += 1;
        EXTRACT_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(CachedCounter(*state))
    }
}

#[test]
fn runtime_caches_system_param_state_across_runs() {
    fn cached(counter: CachedCounter, mut frame: ResMut<Frame>) {
        frame.0 += counter.0 as u64;
    }

    let init_before = INIT_STATE_CALLS.load(Ordering::SeqCst);
    let extract_before = EXTRACT_CALLS.load(Ordering::SeqCst);

    let mut world = World::new();
    world.insert_resource(Frame(0));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, cached);
    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(
        INIT_STATE_CALLS.load(Ordering::SeqCst) - init_before,
        1,
        "state should initialize exactly once per registered system",
    );
    assert_eq!(
        EXTRACT_CALLS.load(Ordering::SeqCst) - extract_before,
        2,
        "state should extract once per run",
    );
    assert_eq!(world.resource::<Frame>().unwrap().0, 3);
}

#[test]
fn runtime_reports_extraction_errors_cleanly() {
    fn requires_missing_resource(_missing: Res<MissingRes>) {}

    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, requires_missing_resource);

    let err = runtime.run_schedule::<Update>(&mut world).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("runtime setup failed"), "{message}");
    assert!(message.contains("does not exist"), "{message}");
}

#[test]
fn res_provides_read_only_resource_access() {
    fn mirror_frame_into_score(frame: Res<Frame>, mut score: ResMut<Score>) {
        score.0 = frame.0;
    }

    let mut world = World::new();
    world.insert_resource(Frame(42));
    world.insert_resource(Score(0));
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, mirror_frame_into_score);
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<Frame>().unwrap().0, 42);
    assert_eq!(world.resource::<Score>().unwrap().0, 42);
}

#[test]
fn commands_flush_at_stage_end_not_between_systems_in_same_stage() {
    fn enqueue_spawn(mut commands: Commands) {
        commands.spawn(Marker);
    }

    fn observe_marker_count(mut seen: ResMut<SeenCount>, mut query: Query<&Marker>) {
        seen.0 = query.iter().count() as u32;
    }

    let mut world = World::new();
    world.insert_resource(SeenCount(99));

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (enqueue_spawn, observe_marker_count));
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<SeenCount>().unwrap().0, 0);
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 1);
}
