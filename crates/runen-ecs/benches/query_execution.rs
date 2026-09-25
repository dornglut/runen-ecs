use criterion::{Criterion, criterion_group, criterion_main};
use runen_ecs::prelude::*;
use std::hint::black_box;

const QUERY_ENTITY_COUNT: usize = 10_000;

#[derive(Debug, Copy, Clone, Component)]
struct Position(u32);

#[derive(Debug, Copy, Clone, Component)]
struct Velocity(u32);

#[derive(ScheduleLabel)]
struct Measure;

fn build_world(count: usize) -> World {
    let mut world = World::new();
    for index in 0..count {
        world.spawn((Position(index as u32), Velocity(1))).unwrap();
    }
    world
}

fn position_checksum(world: &World) -> u64 {
    world
        .query::<&Position>()
        .iter(world)
        .map(|position| u64::from(position.0))
        .sum()
}

fn no_op() {}

fn observe_positions(mut query: Query<&Position>) {
    let checksum = query
        .iter()
        .map(|position| u64::from(position.0))
        .sum::<u64>();
    black_box(checksum);
}

fn increment_positions(mut query: Query<&mut Position>) {
    let mut checksum = 0_u64;
    for position in query.iter() {
        position.0 = position.0.wrapping_add(1);
        checksum = checksum.wrapping_add(u64::from(position.0));
    }
    black_box(checksum);
}

fn no_op_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime.add_systems(Measure, no_op).unwrap();
    runtime.validate().unwrap();
    runtime
}

fn read_only_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime.add_systems(Measure, observe_positions).unwrap();
    runtime.validate().unwrap();
    runtime
}

fn mutable_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime.add_systems(Measure, increment_positions).unwrap();
    runtime.validate().unwrap();
    runtime
}

fn prove_fixture_semantics() {
    const COUNT: usize = 16;
    let initial_checksum = (0..COUNT as u64).sum::<u64>();

    let read_world = build_world(COUNT);
    assert_eq!(position_checksum(&read_world), initial_checksum);

    let mut direct_mutable_world = build_world(COUNT);
    let direct_mutable_query = direct_mutable_world.query::<&mut Position>();
    assert_eq!(
        direct_mutable_query.iter(&mut direct_mutable_world).count(),
        COUNT
    );
    assert_eq!(
        position_checksum(&direct_mutable_world),
        initial_checksum,
        "merely yielding &mut values preserves payloads while still exercising change bookkeeping"
    );

    let mut direct_mixed_world = build_world(COUNT);
    let direct_mixed_query = direct_mixed_world.query::<(&mut Position, &Velocity)>();
    for (position, velocity) in direct_mixed_query.iter(&mut direct_mixed_world) {
        position.0 = position.0.wrapping_add(velocity.0);
    }
    assert_eq!(
        position_checksum(&direct_mixed_world),
        initial_checksum + COUNT as u64
    );

    let mut serial_world = build_world(COUNT);
    let mut serial_runtime = mutable_runtime();
    serial_runtime
        .run_schedule::<Measure>(&mut serial_world)
        .unwrap();
    assert_eq!(
        position_checksum(&serial_world),
        initial_checksum + COUNT as u64
    );

    let mut parallel_world = build_world(COUNT);
    let mut parallel_runtime = mutable_runtime();
    parallel_runtime
        .run_schedule_parallel::<Measure>(&mut parallel_world, 1)
        .unwrap();
    assert_eq!(
        position_checksum(&parallel_world),
        initial_checksum + COUNT as u64
    );
}

fn bench_direct_read_only(c: &mut Criterion) {
    prove_fixture_semantics();
    let world = build_world(QUERY_ENTITY_COUNT);
    let query = world.query::<&Position>();

    c.bench_function("direct_serial_read_only_query_10000", |b| {
        b.iter(|| {
            let checksum = query
                .iter(&world)
                .map(|position| u64::from(position.0))
                .sum::<u64>();
            black_box(checksum);
        });
    });
}

fn bench_direct_mutable(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let query = world.query::<&mut Position>();

    c.bench_function("direct_serial_mutable_query_10000", |b| {
        b.iter(|| {
            let mut checksum = 0_u64;
            for position in query.iter(&mut world) {
                position.0 = position.0.wrapping_add(1);
                checksum = checksum.wrapping_add(u64::from(position.0));
            }
            black_box(checksum);
        });
    });
}

fn bench_direct_mutable_read(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let query = world.query::<(&mut Position, &Velocity)>();

    c.bench_function("direct_serial_mutable_read_query_10000", |b| {
        b.iter(|| {
            let mut checksum = 0_u64;
            for (position, velocity) in query.iter(&mut world) {
                position.0 = position.0.wrapping_add(velocity.0);
                checksum = checksum.wrapping_add(u64::from(position.0));
            }
            black_box(checksum);
        });
    });
}

fn bench_serial_no_op_schedule(c: &mut Criterion) {
    let mut world = World::new();
    let mut runtime = no_op_runtime();

    c.bench_function("serial_schedule_no_op", |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap());
    });
}

fn bench_serial_read_only_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = read_only_runtime();

    c.bench_function("serial_schedule_read_only_query_10000", |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap());
    });
}

fn bench_serial_mutable_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = mutable_runtime();

    c.bench_function("serial_schedule_mutable_query_10000", |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap());
    });
}

fn bench_parallel_no_op_schedule(c: &mut Criterion) {
    let mut world = World::new();
    let mut runtime = no_op_runtime();

    c.bench_function("parallel_schedule_no_op_worker_1", |b| {
        b.iter(|| {
            runtime
                .run_schedule_parallel::<Measure>(&mut world, 1)
                .unwrap()
        });
    });
}

fn bench_parallel_read_only_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = read_only_runtime();

    c.bench_function("parallel_schedule_read_only_query_10000_worker_1", |b| {
        b.iter(|| {
            runtime
                .run_schedule_parallel::<Measure>(&mut world, 1)
                .unwrap()
        });
    });
}

fn bench_parallel_mutable_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = mutable_runtime();

    c.bench_function("parallel_schedule_mutable_query_10000_worker_1", |b| {
        b.iter(|| {
            runtime
                .run_schedule_parallel::<Measure>(&mut world, 1)
                .unwrap()
        });
    });
}

criterion_group!(
    query_execution,
    bench_direct_read_only,
    bench_direct_mutable,
    bench_direct_mutable_read,
    bench_serial_no_op_schedule,
    bench_serial_read_only_schedule,
    bench_serial_mutable_schedule,
    bench_parallel_no_op_schedule,
    bench_parallel_read_only_schedule,
    bench_parallel_mutable_schedule,
);
criterion_main!(query_execution);
