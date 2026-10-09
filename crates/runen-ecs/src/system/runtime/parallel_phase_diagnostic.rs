//! Release-only cohort phase diagnostic for #147.
//!
//! Uses the accepted public eight-column independent-writer workload, with
//! World construction and verification outside timed schedule invocations.
use crate::prelude::*;
use crate::system::worker_cohort::phase_probe;
use std::env;
use std::hint::black_box;
use std::time::Instant;

#[derive(ScheduleLabel)]
struct Diagnostic;

macro_rules! component_and_writer {
    ($component:ident, $writer:ident) => {
        #[derive(Component)]
        struct $component(u64);

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

component_and_writer!(C0, write_0);
component_and_writer!(C1, write_1);
component_and_writer!(C2, write_2);
component_and_writer!(C3, write_3);
component_and_writer!(C4, write_4);
component_and_writer!(C5, write_5);
component_and_writer!(C6, write_6);
component_and_writer!(C7, write_7);

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

fn sizes() -> Vec<usize> {
    match env::var("RUNEN_ECS_PHASE_DIAGNOSTIC_SIZES") {
        Err(env::VarError::NotPresent) => vec![10_000],
        Ok(value) => match value.as_str() {
            "10000" => vec![10_000],
            "100000" => vec![100_000],
            "1000000" => vec![1_000_000],
            "all" => vec![10_000, 100_000, 1_000_000],
            _ => panic!("RUNEN_ECS_PHASE_DIAGNOSTIC_SIZES must be 10000, 100000, 1000000 or all"),
        },
        Err(error) => panic!("invalid RUNEN_ECS_PHASE_DIAGNOSTIC_SIZES: {error}"),
    }
}

fn fixture(count: usize) -> (World, Runtime) {
    let mut world = World::new();
    for index in 0..count {
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
    let mut runtime = Runtime::new();
    runtime
        .add_systems(
            Diagnostic,
            (
                write_0, write_1, write_2, write_3,
                write_4, write_5, write_6, write_7,
            ),
        )
        .unwrap();
    runtime.validate().unwrap();
    (world, runtime)
}

fn verify_rows(world: &World, count: usize, rounds: u64) {
    let n = count as u128;
    let r = rounds as u128;
    let first = world
        .query::<&C0>()
        .iter(world)
        .map(|value| value.0 as u128)
        .sum::<u128>();
    assert_eq!(first, (n * (n - 1)) / 2 + n * r);
    macro_rules! assert_column {
        ($component:ident, $starting:expr) => {
            let actual = world
                .query::<&$component>()
                .iter(world)
                .map(|value| value.0 as u128)
                .sum::<u128>();
            assert_eq!(actual, n * ($starting as u128 + r));
        };
    }
    assert_column!(C1, 1);
    assert_column!(C2, 2);
    assert_column!(C3, 3);
    assert_column!(C4, 4);
    assert_column!(C5, 5);
    assert_column!(C6, 6);
    assert_column!(C7, 7);
}

#[test]
#[ignore = "bounded release-only cohort decomposition; run the dedicated hosted diagnostic"]
fn parallel_phase_scaling_diagnostic() {
    if cfg!(debug_assertions) {
        panic!("diagnostic requires release optimizations");
    }
    for count in sizes() {
        for workers in [0_usize, 1, 2, 4] {
            // Reuse one initialized World and Runtime across the three timed
            // invocations. World construction, schedule validation, and final
            // component-value verification are intentionally outside timing.
            let (mut world, mut runtime) = fixture(count);
            for round in 0..3_u64 {
                phase_probe::begin();
                let started = Instant::now();
                if workers == 0 {
                    runtime.run_schedule::<Diagnostic>(&mut world).unwrap();
                } else {
                    runtime
                        .run_schedule_parallel::<Diagnostic>(&mut world, workers)
                        .unwrap();
                }
                let total_ns = started.elapsed().as_nanos();
                let phases = phase_probe::finish();
                assert!(phases.prepare_ns + phases.execute_ns + phases.reconcile_ns <= total_ns);
                if workers == 0 {
                    assert_eq!(phases.cohorts, 0);
                } else {
                    assert!(phases.cohorts > 0);
                }
                println!(
                    "ecs_phase_sample,entities={count},workers={workers},round={round},total_ns={total_ns},prepare_ns={},execute_ns={},reconcile_ns={},cohorts={}",
                    phases.prepare_ns,
                    phases.execute_ns,
                    phases.reconcile_ns,
                    phases.cohorts
                );
            }
            verify_rows(&world, count, 3);
        }
    }
}
