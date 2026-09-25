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

#[derive(Debug, Copy, Clone)]
enum LookupTarget {
    Early,
    Middle,
    Late,
    NonMember,
}

struct LookupFixture {
    world: World,
    early: Entity,
    middle: Entity,
    late: Entity,
    non_member: Entity,
}

impl LookupFixture {
    fn target(&self, target: LookupTarget) -> Entity {
        match target {
            LookupTarget::Early => self.early,
            LookupTarget::Middle => self.middle,
            LookupTarget::Late => self.late,
            LookupTarget::NonMember => self.non_member,
        }
    }
}

fn build_lookup_world(count: usize) -> LookupFixture {
    assert!(count >= 3, "lookup benchmark requires early, middle, and late rows");

    let middle_index = count / 2;
    let mut world = World::new();
    let mut early = None;
    let mut middle = None;
    let mut late = None;

    for index in 0..count {
        let entity = world
            .spawn((Position(index as u32), Velocity(1)))
            .unwrap();
        if index == 0 {
            early = Some(entity);
        }
        if index == middle_index {
            middle = Some(entity);
        }
        if index + 1 == count {
            late = Some(entity);
        }
    }

    let early = early.expect("lookup fixture must contain an early row");
    let middle = middle.expect("lookup fixture must contain a middle row");
    let late = late.expect("lookup fixture must contain a late row");
    let non_member = world
        .spawn((Position(count as u32), Velocity(1)))
        .unwrap();
    world.despawn(non_member).unwrap();

    let query = world.query::<&Position>();
    assert_eq!(query.get(&world, early).map(|position| position.0), Some(0));
    assert_eq!(
        query.get(&world, middle).map(|position| position.0),
        Some(middle_index as u32)
    );
    assert_eq!(
        query.get(&world, late).map(|position| position.0),
        Some((count - 1) as u32)
    );
    assert!(query.get(&world, non_member).is_none());

    LookupFixture {
        world,
        early,
        middle,
        late,
        non_member,
    }
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

fn receive_read_only_query(_query: Query<&Position>) {}

fn receive_mutable_query(_query: Query<&mut Position>) {}

fn consume_mutable_positions(mut query: Query<&mut Position>) {
    black_box(query.iter().count());
}

fn consume_double_mutable_positions(mut query: Query<(&mut Position, &mut Velocity)>) {
    black_box(query.iter().count());
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

fn read_only_parameter_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime
        .add_systems(Measure, receive_read_only_query)
        .unwrap();
    runtime.validate().unwrap();
    runtime
}

fn read_only_get_runtime(entity: Entity) -> Runtime {
    let mut runtime = Runtime::new();
    runtime
        .add_systems(Measure, move |mut query: Query<&Position>| {
            let _ = black_box(query.get(entity).map(|position| position.0));
        })
        .unwrap();
    runtime.validate().unwrap();
    runtime
}

fn mutable_parameter_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime.add_systems(Measure, receive_mutable_query).unwrap();
    runtime.validate().unwrap();
    runtime
}

fn mutable_yield_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime
        .add_systems(Measure, consume_mutable_positions)
        .unwrap();
    runtime.validate().unwrap();
    runtime
}

fn double_mutable_yield_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime
        .add_systems(Measure, consume_double_mutable_positions)
        .unwrap();
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

    let mut parameter_read_world = build_world(COUNT);
    let mut parameter_read_runtime = read_only_parameter_runtime();
    parameter_read_runtime
        .run_schedule_parallel::<Measure>(&mut parameter_read_world, 1)
        .unwrap();
    assert_eq!(position_checksum(&parameter_read_world), initial_checksum);

    let mut parameter_mutable_world = build_world(COUNT);
    let mut parameter_mutable_runtime = mutable_parameter_runtime();
    parameter_mutable_runtime
        .run_schedule_parallel::<Measure>(&mut parameter_mutable_world, 1)
        .unwrap();
    assert_eq!(
        position_checksum(&parameter_mutable_world),
        initial_checksum,
        "parameter-only mutable query preparation must not mutate payloads"
    );

    let mut serial_yield_world = build_world(COUNT);
    let mut serial_yield_runtime = mutable_yield_runtime();
    serial_yield_runtime
        .run_schedule::<Measure>(&mut serial_yield_world)
        .unwrap();
    assert_eq!(
        position_checksum(&serial_yield_world),
        initial_checksum,
        "yield-only serial mutable queries must not alter payload values"
    );

    let mut parallel_yield_world = build_world(COUNT);
    let mut parallel_yield_runtime = mutable_yield_runtime();
    parallel_yield_runtime
        .run_schedule_parallel::<Measure>(&mut parallel_yield_world, 1)
        .unwrap();
    assert_eq!(
        position_checksum(&parallel_yield_world),
        initial_checksum,
        "yield-only worker mutable queries must not alter payload values"
    );

    let mut double_yield_world = build_world(COUNT);
    let mut double_yield_runtime = double_mutable_yield_runtime();
    double_yield_runtime
        .run_schedule::<Measure>(&mut double_yield_world)
        .unwrap();
    assert_eq!(
        position_checksum(&double_yield_world),
        initial_checksum,
        "yield-only double-mutable queries must not alter payload values"
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

fn bench_direct_mutable_yield_only(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let query = world.query::<&mut Position>();

    c.bench_function("direct_serial_mutable_yield_only_10000", |b| {
        b.iter(|| black_box(query.iter(&mut world).count()));
    });
}

fn bench_direct_double_mutable_yield_only(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let query = world.query::<(&mut Position, &mut Velocity)>();

    c.bench_function("direct_serial_double_mutable_yield_only_10000", |b| {
        b.iter(|| black_box(query.iter(&mut world).count()));
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

fn bench_serial_read_only_parameter_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = read_only_parameter_runtime();

    c.bench_function("serial_schedule_read_only_query_parameter_only_10000", |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap())
    });
}

fn bench_serial_read_only_get_case(
    c: &mut Criterion,
    name: &'static str,
    target: LookupTarget,
) {
    let fixture = build_lookup_world(QUERY_ENTITY_COUNT);
    let entity = fixture.target(target);
    let mut world = fixture.world;
    let mut runtime = read_only_get_runtime(entity);

    c.bench_function(name, |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap())
    });
}

fn bench_serial_read_only_get(c: &mut Criterion) {
    bench_serial_read_only_get_case(
        c,
        "serial_schedule_read_only_query_get_early_10000",
        LookupTarget::Early,
    );
    bench_serial_read_only_get_case(
        c,
        "serial_schedule_read_only_query_get_middle_10000",
        LookupTarget::Middle,
    );
    bench_serial_read_only_get_case(
        c,
        "serial_schedule_read_only_query_get_late_10000",
        LookupTarget::Late,
    );
    bench_serial_read_only_get_case(
        c,
        "serial_schedule_read_only_query_get_non_member_10000",
        LookupTarget::NonMember,
    );
}

fn bench_serial_mutable_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = mutable_runtime();

    c.bench_function("serial_schedule_mutable_query_10000", |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap());
    });
}

fn bench_serial_mutable_parameter_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = mutable_parameter_runtime();

    c.bench_function("serial_schedule_mutable_query_parameter_only_10000", |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap())
    });
}

fn bench_serial_mutable_yield_only_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = mutable_yield_runtime();

    c.bench_function("serial_schedule_mutable_yield_only_10000", |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap());
    });
}

fn bench_serial_double_mutable_yield_only_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = double_mutable_yield_runtime();

    c.bench_function("serial_schedule_double_mutable_yield_only_10000", |b| {
        b.iter(|| runtime.run_schedule::<Measure>(&mut world).unwrap())
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

fn bench_parallel_read_only_parameter_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = read_only_parameter_runtime();

    c.bench_function(
        "parallel_schedule_read_only_query_parameter_only_10000_worker_1",
        |b| {
            b.iter(|| {
                runtime
                    .run_schedule_parallel::<Measure>(&mut world, 1)
                    .unwrap()
            });
        },
    );
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

fn bench_parallel_read_only_get_case(
    c: &mut Criterion,
    name: &'static str,
    target: LookupTarget,
) {
    let fixture = build_lookup_world(QUERY_ENTITY_COUNT);
    let entity = fixture.target(target);
    let mut world = fixture.world;
    let mut runtime = read_only_get_runtime(entity);

    c.bench_function(name, |b| {
        b.iter(|| {
            runtime
                .run_schedule_parallel::<Measure>(&mut world, 1)
                .unwrap()
        })
    });
}

fn bench_parallel_read_only_get(c: &mut Criterion) {
    bench_parallel_read_only_get_case(
        c,
        "parallel_schedule_read_only_query_get_early_10000_worker_1",
        LookupTarget::Early,
    );
    bench_parallel_read_only_get_case(
        c,
        "parallel_schedule_read_only_query_get_middle_10000_worker_1",
        LookupTarget::Middle,
    );
    bench_parallel_read_only_get_case(
        c,
        "parallel_schedule_read_only_query_get_late_10000_worker_1",
        LookupTarget::Late,
    );
    bench_parallel_read_only_get_case(
        c,
        "parallel_schedule_read_only_query_get_non_member_10000_worker_1",
        LookupTarget::NonMember,
    );
}

fn bench_parallel_mutable_parameter_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = mutable_parameter_runtime();

    c.bench_function(
        "parallel_schedule_mutable_query_parameter_only_10000_worker_1",
        |b| {
            b.iter(|| {
                runtime
                    .run_schedule_parallel::<Measure>(&mut world, 1)
                    .unwrap()
            });
        },
    );
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

fn bench_parallel_mutable_yield_only_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = mutable_yield_runtime();

    c.bench_function("parallel_schedule_mutable_yield_only_10000_worker_1", |b| {
        b.iter(|| {
            runtime
                .run_schedule_parallel::<Measure>(&mut world, 1)
                .unwrap()
        });
    });
}

fn bench_parallel_double_mutable_yield_only_schedule(c: &mut Criterion) {
    let mut world = build_world(QUERY_ENTITY_COUNT);
    let mut runtime = double_mutable_yield_runtime();

    c.bench_function(
        "parallel_schedule_double_mutable_yield_only_10000_worker_1",
        |b| {
            b.iter(|| {
                runtime
                    .run_schedule_parallel::<Measure>(&mut world, 1)
                    .unwrap()
            });
        },
    );
}

criterion_group!(
    query_execution,
    bench_direct_read_only,
    bench_direct_mutable,
    bench_direct_mutable_yield_only,
    bench_direct_double_mutable_yield_only,
    bench_direct_mutable_read,
    bench_serial_no_op_schedule,
    bench_serial_read_only_schedule,
    bench_serial_read_only_parameter_schedule,
    bench_serial_read_only_get,
    bench_serial_mutable_schedule,
    bench_serial_mutable_parameter_schedule,
    bench_serial_mutable_yield_only_schedule,
    bench_serial_double_mutable_yield_only_schedule,
    bench_parallel_no_op_schedule,
    bench_parallel_read_only_parameter_schedule,
    bench_parallel_read_only_schedule,
    bench_parallel_read_only_get,
    bench_parallel_mutable_parameter_schedule,
    bench_parallel_mutable_schedule,
    bench_parallel_mutable_yield_only_schedule,
    bench_parallel_double_mutable_yield_only_schedule,
);
criterion_main!(query_execution);
