use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use runen_ecs::prelude::*;
use std::env;
use std::hint::black_box;

#[derive(ScheduleLabel)]
struct Measure;

macro_rules! component_systems {
    ($component:ident, $reader:ident, $writer:ident) => {
        #[derive(Component)]
        struct $component(u64);

        fn $reader(mut query: Query<&$component>) {
            let sum = query
                .iter()
                .fold(0_u64, |sum, value| sum.wrapping_add(value.0));
            black_box(sum);
        }

        fn $writer(mut query: Query<&mut $component>) {
            let mut sum = 0_u64;
            for value in query.iter() {
                value.0 = value.0.wrapping_add(1);
                sum = sum.wrapping_add(value.0);
            }
            black_box(sum);
        }
    };
}

component_systems!(C0, read_0, write_0);
component_systems!(C1, read_1, write_1);
component_systems!(C2, read_2, write_2);
component_systems!(C3, read_3, write_3);
component_systems!(C4, read_4, write_4);
component_systems!(C5, read_5, write_5);
component_systems!(C6, read_6, write_6);
component_systems!(C7, read_7, write_7);

#[derive(Bundle)]
struct FullRow {
    c0: C0,
    c1: C1,
    c2: C2,
    c3: C3,
    c4: C4,
    c5: C5,
    c6: C6,
    c7: C7,
}

macro_rules! conflicting_writer {
    ($name:ident) => {
        fn $name(mut query: Query<&mut C0>) {
            let mut sum = 0_u64;
            for value in query.iter() {
                value.0 = value.0.wrapping_add(1);
                sum = sum.wrapping_add(value.0);
            }
            black_box(sum);
        }
    };
}

conflicting_writer!(conflict_1);
conflicting_writer!(conflict_2);
conflicting_writer!(conflict_3);
conflicting_writer!(conflict_4);
conflicting_writer!(conflict_5);
conflicting_writer!(conflict_6);
conflicting_writer!(conflict_7);

macro_rules! empty_system {
    ($name:ident) => {
        fn $name() {}
    };
}

empty_system!(empty_0);
empty_system!(empty_1);
empty_system!(empty_2);
empty_system!(empty_3);
empty_system!(empty_4);
empty_system!(empty_5);
empty_system!(empty_6);
empty_system!(empty_7);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Workload {
    Empty,
    Reads,
    IndependentWrites,
    ConflictingWrites,
}

impl Workload {
    fn label(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Reads => "reads",
            Self::IndependentWrites => "independent_writes",
            Self::ConflictingWrites => "conflicting_writes",
        }
    }

    fn register(self, runtime: &mut Runtime) {
        match self {
            Self::Empty => runtime
                .add_systems(
                    Measure,
                    (
                        empty_0, empty_1, empty_2, empty_3, empty_4, empty_5, empty_6, empty_7,
                    ),
                )
                .unwrap(),
            Self::Reads => runtime
                .add_systems(
                    Measure,
                    (read_0, read_1, read_2, read_3, read_4, read_5, read_6, read_7),
                )
                .unwrap(),
            Self::IndependentWrites => runtime
                .add_systems(
                    Measure,
                    (
                        write_0, write_1, write_2, write_3, write_4, write_5, write_6, write_7,
                    ),
                )
                .unwrap(),
            Self::ConflictingWrites => runtime
                .add_systems(
                    Measure,
                    (
                        write_0, conflict_1, conflict_2, conflict_3, conflict_4, conflict_5,
                        conflict_6, conflict_7,
                    ),
                )
                .unwrap(),
        };
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Serial,
    Parallel(usize),
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::Parallel(1) => "worker_1",
            Self::Parallel(2) => "worker_2",
            Self::Parallel(4) => "worker_4",
            Self::Parallel(8) => "worker_8",
            Self::Parallel(_) => unreachable!("unsupported benchmark worker capacity"),
        }
    }
}

fn build_world(entity_count: usize) -> World {
    let mut world = World::new();
    for index in 0..entity_count {
        world
            .spawn(FullRow {
                c0: C0(index as u64),
                c1: C1(1),
                c2: C2(2),
                c3: C3(3),
                c4: C4(4),
                c5: C5(5),
                c6: C6(6),
                c7: C7(7),
            })
            .unwrap();
    }
    world
}

struct Fixture {
    world: World,
    runtime: Runtime,
}

impl Fixture {
    fn new(entity_count: usize, workload: Workload) -> Self {
        let world = build_world(entity_count);
        let mut runtime = Runtime::new();
        workload.register(&mut runtime);
        runtime.validate().unwrap();
        Self { world, runtime }
    }

    fn run(&mut self, mode: Mode) {
        match mode {
            Mode::Serial => self.runtime.run_schedule::<Measure>(&mut self.world).unwrap(),
            Mode::Parallel(capacity) => self
                .runtime
                .run_schedule_parallel::<Measure>(&mut self.world, capacity)
                .unwrap(),
        }
    }
}

fn component_sums(world: &World) -> [u64; 8] {
    [
        world.query::<&C0>().iter(world).map(|v| v.0).sum(),
        world.query::<&C1>().iter(world).map(|v| v.0).sum(),
        world.query::<&C2>().iter(world).map(|v| v.0).sum(),
        world.query::<&C3>().iter(world).map(|v| v.0).sum(),
        world.query::<&C4>().iter(world).map(|v| v.0).sum(),
        world.query::<&C5>().iter(world).map(|v| v.0).sum(),
        world.query::<&C6>().iter(world).map(|v| v.0).sum(),
        world.query::<&C7>().iter(world).map(|v| v.0).sum(),
    ]
}

/// Untimed public-API equivalence proof for all workload and capacity variants.
fn prove_fixture_semantics() {
    for workload in [
        Workload::Empty,
        Workload::Reads,
        Workload::IndependentWrites,
        Workload::ConflictingWrites,
    ] {
        let count = if workload == Workload::Empty { 0 } else { 16 };
        for capacity in [1, 2, 4, 8] {
            let mut serial = Fixture::new(count, workload);
            let mut parallel = Fixture::new(count, workload);
            let original = component_sums(&serial.world);
            assert_eq!(original, component_sums(&parallel.world));
            for round in 1..=3 {
                serial.run(Mode::Serial);
                parallel.run(Mode::Parallel(capacity));
                let result = component_sums(&serial.world);
                assert_eq!(
                    result,
                    component_sums(&parallel.world),
                    "serial/parallel payload difference: {workload:?}, capacity={capacity}, round={round}"
                );
                match workload {
                    Workload::Empty | Workload::Reads => assert_eq!(result, original),
                    Workload::IndependentWrites => {
                        for (actual, initial) in result.iter().zip(original) {
                            assert_eq!(*actual, initial + count as u64 * round);
                        }
                    }
                    Workload::ConflictingWrites => {
                        assert_eq!(result[0], original[0] + 8 * count as u64 * round);
                        assert_eq!(&result[1..], &original[1..]);
                    }
                }
            }
        }
    }
}

fn selected_entity_counts() -> Vec<usize> {
    match env::var("RUNEN_ECS_BENCH_SIZE") {
        Err(env::VarError::NotPresent) => vec![1_000],
        Ok(value) => match value.as_str() {
            "1000" => vec![1_000],
            "10000" => vec![10_000],
            "100000" => vec![100_000],
            "1000000" => vec![1_000_000],
            "all" => vec![1_000, 10_000, 100_000, 1_000_000],
            _ => panic!("RUNEN_ECS_BENCH_SIZE must be 1000, 10000, 100000, 1000000, or all"),
        },
        Err(error) => panic!("invalid RUNEN_ECS_BENCH_SIZE: {error}"),
    }
}

fn bench_case(c: &mut Criterion, workload: Workload, entity_count: usize) {
    let mut group = c.benchmark_group(format!(
        "parallel_scaling/{}/{entity_count}",
        workload.label()
    ));
    if workload != Workload::Empty {
        // Each of eight systems visits exactly entity_count component values.
        // This is component visits, not distinct entities or bytes transferred.
        group.throughput(Throughput::Elements(entity_count as u64 * 8));
    }

    for mode in [
        Mode::Serial,
        Mode::Parallel(1),
        Mode::Parallel(2),
        Mode::Parallel(4),
        Mode::Parallel(8),
    ] {
        // One fixture per measured variant; drop it before constructing the next
        // one. World construction and schedule-plan validation are outside timing.
        let mut fixture = Fixture::new(entity_count, workload);
        group.bench_function(mode.label(), |b| b.iter(|| fixture.run(mode)));
    }
    group.finish();
}

fn bench_parallel_scaling(c: &mut Criterion) {
    prove_fixture_semantics();

    // Dispatch has eight systems and no entities: it must not be mislabeled as
    // an entity-throughput benchmark.
    bench_case(c, Workload::Empty, 0);
    for entity_count in selected_entity_counts() {
        for workload in [
            Workload::Reads,
            Workload::IndependentWrites,
            Workload::ConflictingWrites,
        ] {
            bench_case(c, workload, entity_count);
        }
    }
}

criterion_group!(parallel_scaling, bench_parallel_scaling);
criterion_main!(parallel_scaling);
