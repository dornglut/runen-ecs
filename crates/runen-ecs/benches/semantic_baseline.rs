use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use runen_ecs::QueryState;
use runen_ecs::prelude::*;
use std::any::TypeId;
use std::hint::black_box;

const ENTITY_COMPONENT_COUNT: usize = 1000;
const QUERY_ENTITY_COUNT: usize = 10_000;

#[derive(Debug, Copy, Clone, Component)]
struct Position(u32);

#[derive(Debug, Copy, Clone, Component)]
struct Velocity(u32);

#[derive(Debug, Copy, Clone, Component)]
struct TransitionMarker;

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

fn build_world(count: usize) -> World {
    let mut world = World::new();
    for index in 0..count {
        world.spawn((Position(index as u32), Velocity(1))).unwrap();
    }
    world
}

fn bench_entity_component_insertion(c: &mut Criterion) {
    c.bench_function("entity_component_insertion_1000", |b| {
        b.iter_batched_ref(World::new, insert_entities, BatchSize::SmallInput);
    });
}

fn insert_entities(world: &mut World) {
    for index in 0..ENTITY_COMPONENT_COUNT {
        world.spawn((Position(index as u32), Velocity(1))).unwrap();
    }
    black_box(world);
}

fn bench_query_iteration(c: &mut Criterion) {
    let world = build_world(QUERY_ENTITY_COUNT);
    let query = world.query::<(&Position, &Velocity)>();
    c.bench_function("scalar_query_iteration_10000", |b| {
        b.iter(|| {
            let checksum = query
                .iter(&world)
                .map(|(position, velocity)| u64::from(position.0 + velocity.0))
                .sum::<u64>();
            black_box(checksum);
        });
    });
}

fn contiguous_query_checksum(query: &QueryState<(&Position, &Velocity)>, world: &World) -> u64 {
    let mut checksum = 0_u64;
    for segment in query
        .try_contiguous_segments(world)
        .expect("the benchmark query has a supported contiguous shape")
    {
        let (positions, velocities) = segment
            .component_pair::<Position, Velocity>()
            .expect("both benchmark components are projected");
        checksum += positions
            .iter()
            .zip(velocities)
            .map(|(position, velocity)| u64::from(position.0 + velocity.0))
            .sum::<u64>();
    }
    checksum
}

fn prove_contiguous_query_fixture() {
    let scalar_world = build_world(QUERY_ENTITY_COUNT);
    let scalar_query = scalar_world.query::<(&Position, &Velocity)>();
    let scalar_checksum = scalar_query
        .iter(&scalar_world)
        .map(|(position, velocity)| u64::from(position.0 + velocity.0))
        .sum::<u64>();

    let contiguous_world = build_world(QUERY_ENTITY_COUNT);
    let contiguous_query = contiguous_world.query::<(&Position, &Velocity)>();
    assert_eq!(
        contiguous_query_checksum(&contiguous_query, &contiguous_world),
        scalar_checksum
    );
}

fn bench_contiguous_query_iteration(c: &mut Criterion) {
    prove_contiguous_query_fixture();
    let world = build_world(QUERY_ENTITY_COUNT);
    let query = world.query::<(&Position, &Velocity)>();
    c.bench_function("contiguous_query_iteration_10000", |b| {
        b.iter(|| black_box(contiguous_query_checksum(&query, &world)));
    });
}

struct TransitionFixture {
    world: World,
    entities: Vec<Entity>,
}

fn build_transition_fixture() -> TransitionFixture {
    let mut world = World::new();
    let mut entities = Vec::with_capacity(ENTITY_COMPONENT_COUNT);
    for index in 0..ENTITY_COMPONENT_COUNT {
        entities.push(world.spawn((Position(index as u32), Velocity(1))).unwrap());
    }
    TransitionFixture { world, entities }
}

fn apply_transition(fixture: &mut TransitionFixture) {
    for entity in fixture.entities.iter().copied() {
        fixture.world.insert(entity, TransitionMarker).unwrap();
    }
    black_box(&mut fixture.world);
}

fn assert_transition_fixture(fixture: &TransitionFixture, marker_present: bool) {
    assert_eq!(fixture.entities.len(), ENTITY_COMPONENT_COUNT);
    assert_eq!(
        fixture
            .world
            .query::<(&Position, &Velocity)>()
            .iter(&fixture.world)
            .count(),
        ENTITY_COMPONENT_COUNT
    );
    assert!(fixture.entities.iter().copied().all(|entity| {
        fixture
            .world
            .entity_has_component_type(entity, TypeId::of::<TransitionMarker>())
            == marker_present
    }));
}

fn prove_transition_fixture() {
    let mut fixture = build_transition_fixture();
    assert_transition_fixture(&fixture, false);
    apply_transition(&mut fixture);
    assert_transition_fixture(&fixture, true);
}

fn bench_archetype_transition(c: &mut Criterion) {
    prove_transition_fixture();
    c.bench_function("archetype_transition_1000", |b| {
        b.iter_batched_ref(
            build_transition_fixture,
            apply_transition,
            BatchSize::SmallInput,
        );
    });
}

fn increment(mut query: Query<&mut Position>) {
    for position in query.iter() {
        position.0 += 1;
    }
}

fn bench_serial_schedule_execution(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = Runtime::new();
    runtime.add_systems(Update, increment).unwrap();
    c.bench_function("serial_schedule_execution_10000", |b| {
        b.iter(|| runtime.run_schedule::<Update>(&mut world).unwrap());
    });
}

fn queue_spawn(mut commands: Commands) {
    commands.spawn((Position(3), Velocity(4)));
}

struct DeferredFixture {
    world: World,
    runtime: Runtime,
}

fn build_deferred_fixture() -> DeferredFixture {
    let mut runtime = Runtime::new();
    runtime.add_systems(Update, queue_spawn).unwrap();
    runtime.validate().unwrap();
    DeferredFixture {
        world: World::new(),
        runtime,
    }
}

fn run_deferred_once(fixture: &mut DeferredFixture) {
    fixture
        .runtime
        .run_schedule::<Update>(&mut fixture.world)
        .unwrap();
    black_box(&mut fixture.world);
}

fn count_position_velocity(world: &World) -> usize {
    world.query::<(&Position, &Velocity)>().iter(world).count()
}

fn prove_deferred_fixture() {
    let mut fixture = build_deferred_fixture();
    assert_eq!(count_position_velocity(&fixture.world), 0);
    run_deferred_once(&mut fixture);
    assert_eq!(count_position_velocity(&fixture.world), 1);

    let second_fixture = build_deferred_fixture();
    assert_eq!(count_position_velocity(&second_fixture.world), 0);
}

fn bench_deferred_command_application(c: &mut Criterion) {
    prove_deferred_fixture();
    c.bench_function("deferred_command_application", |b| {
        b.iter_batched_ref(
            build_deferred_fixture,
            run_deferred_once,
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    semantic_baseline,
    bench_entity_component_insertion,
    bench_query_iteration,
    bench_contiguous_query_iteration,
    bench_archetype_transition,
    bench_serial_schedule_execution,
    bench_deferred_command_application,
);
criterion_main!(semantic_baseline);
