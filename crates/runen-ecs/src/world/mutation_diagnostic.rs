//! Ignored release-only diagnostic for #143.
//!
//! This is an observation tool, not a candidate admission strategy. In
//! particular, neither the event-vector baseline nor the isolated reservation
//! baseline produces a valid publicly observable World execution.
use super::{
    ConcurrentMutationCapacity, MutationEvent, MutationJournal, PrevalidatedComponentMutationTarget,
};
use crate::{Component, World};
use std::any::TypeId;
use std::env;
use std::hint::black_box;
use std::time::Instant;

#[derive(Component)]
struct Probe;

fn counts() -> Vec<usize> {
    match env::var("RUNEN_ECS_MUTATION_DIAGNOSTIC_SIZES") {
        Err(env::VarError::NotPresent) => vec![10_000],
        Ok(v) => match v.as_str() {
            "10000" => vec![10_000],
            "100000" => vec![100_000],
            "1000000" => vec![1_000_000],
            "all" => vec![10_000, 100_000, 1_000_000],
            _ => {
                panic!("RUNEN_ECS_MUTATION_DIAGNOSTIC_SIZES must be 10000, 100000, 1000000 or all")
            }
        },
        Err(e) => panic!("invalid RUNEN_ECS_MUTATION_DIAGNOSTIC_SIZES: {e}"),
    }
}

fn target_for(world: &mut World) -> (crate::Entity, TypeId, PrevalidatedComponentMutationTarget) {
    let entity = world.spawn(Probe).unwrap();
    let component_type = TypeId::of::<Probe>();
    let spans = world
        .archetype_registry
        .collect_journal_query_spans(&[component_type], &[], &[component_type], &[component_type])
        .unwrap();
    let changed_tick = spans[0].changed_tick_ptr_at(0, 0).unwrap();
    (
        entity,
        component_type,
        PrevalidatedComponentMutationTarget::new(changed_tick),
    )
}

fn print_sample(size: usize, workers: usize, round: usize, operation: &str, ns: u128) {
    println!(
        "mutation_path_sample,events={size},workers={workers},round={round},operation={operation},nanoseconds={ns}"
    );
}

fn vector_only(size: usize, workers: usize) -> u128 {
    let mut world = World::new();
    let (entity, component_type, target) = target_for(&mut world);
    let each = size / workers;
    let start = Instant::now();
    let vectors = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|_| {
                scope.spawn(move || {
                    let mut events = Vec::new();
                    for _ in 0..each {
                        events.push(MutationEvent::ComponentModified {
                            entity,
                            component_type,
                            target: Some(target),
                        });
                    }
                    black_box(&events);
                    events
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("vector diagnostic worker panicked"))
            .collect::<Vec<_>>()
    });
    let elapsed = start.elapsed().as_nanos();
    assert_eq!(vectors.iter().map(Vec::len).sum::<usize>(), size);
    black_box(vectors);
    elapsed
}

fn reservation_only(size: usize, workers: usize) -> u128 {
    let world = World::new();
    let capacity = ConcurrentMutationCapacity::new(world.current_change_cursor());
    let each = size / workers;
    let start = Instant::now();
    std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|_| {
                let local = capacity.clone();
                scope.spawn(move || {
                    for _ in 0..each {
                        local.reserve_next_event();
                    }
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle
                .join()
                .expect("reservation diagnostic worker panicked");
        }
    });
    let elapsed = start.elapsed().as_nanos();
    assert_eq!(capacity.admitted_for_test(), size as u128);
    elapsed
}

fn serial_record_and_replay(size: usize) -> (u128, u128) {
    let mut world = World::new();
    let (entity, component_type, target) = target_for(&mut world);
    let base = world.current_change_cursor();
    let mut journal = MutationJournal::new(&world);
    let start = Instant::now();
    for _ in 0..size {
        journal.record_prevalidated_component_modified(entity, component_type, target);
    }
    let record_ns = start.elapsed().as_nanos();
    assert_eq!(journal.events.len(), size);
    let start = Instant::now();
    journal.commit(&mut world);
    let replay_ns = start.elapsed().as_nanos();
    assert_eq!(
        world.current_change_cursor().tick(),
        base.tick().checked_add(size as u64).unwrap()
    );
    assert_eq!(
        world
            .archetype_component_metadata::<Probe>(entity)
            .unwrap()
            .1,
        world.current_change_cursor()
    );
    (record_ns, replay_ns)
}

fn concurrent_record_and_replay(
    size: usize,
    workers: usize,
    preallocate: bool,
) -> (u128, u128) {
    let mut world = World::new();
    let (entity, component_type, target) = target_for(&mut world);
    let base = world.current_change_cursor();
    let capacity = ConcurrentMutationCapacity::new(base);
    let each = size / workers;
    let start = Instant::now();
    let journals = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|_| {
                let local = capacity.clone();
                scope.spawn(move || {
                    let mut journal = MutationJournal::new_concurrent(base, local);
                    if preallocate {
                        // This is a diagnostic lower bound, NOT a production
                        // reservation policy: real systems may record an
                        // unknown number of events and can fail mid-invocation.
                        // Include allocation in the timed recording phase.
                        journal.events.reserve_exact(each);
                    }
                    for _ in 0..each {
                        journal.record_prevalidated_component_modified(
                            entity,
                            component_type,
                            target,
                        );
                    }
                    journal
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("journal diagnostic worker panicked"))
            .collect::<Vec<_>>()
    });
    let record_ns = start.elapsed().as_nanos();
    assert_eq!(
        journals
            .iter()
            .map(|journal| journal.events.len())
            .sum::<usize>(),
        size
    );
    assert_eq!(capacity.admitted_for_test(), size as u128);

    // Replay is sequential by reference-rank order, as in the real executor;
    // timing starts *after* worker join and ends before post-state assertions.
    let start = Instant::now();
    for journal in journals {
        journal.commit_concurrent(&mut world);
    }
    let replay_ns = start.elapsed().as_nanos();
    assert_eq!(
        world.current_change_cursor().tick(),
        base.tick().checked_add(size as u64).unwrap()
    );
    assert_eq!(
        world
            .archetype_component_metadata::<Probe>(entity)
            .unwrap()
            .1,
        world.current_change_cursor()
    );
    (record_ns, replay_ns)
}

/// This deliberately does not run in canonical tests or Miri/TSan: it uses wall
/// clock durations and benchmarks the *existing* implementation in a release
/// binary. Every row is a raw sample, not a claimed stable speedup.
#[test]
#[ignore = "run explicitly with --release -- --ignored --nocapture for #143"]
fn mutation_path_scaling_diagnostic() {
    if cfg!(debug_assertions) {
        panic!("diagnostic needs optimized --release");
    }
    for size in counts() {
        for round in 0..3 {
            let (record_ns, replay_ns) = serial_record_and_replay(size);
            print_sample(size, 0, round, "serial_record", record_ns);
            print_sample(size, 0, round, "serial_replay", replay_ns);

            for workers in [1_usize, 2, 4] {
                assert_eq!(size % workers, 0, "equal work division is required");
                print_sample(
                    size,
                    workers,
                    round,
                    "vector_only",
                    vector_only(size, workers),
                );
                print_sample(
                    size,
                    workers,
                    round,
                    "reservation_only",
                    reservation_only(size, workers),
                );
                // Alternate collection order to reduce systematic warm-cache
                // advantage for either the real unreserved path or the
                // hypothetical exact-capacity diagnostic control.
                for preallocate in [round % 2 == 0, round % 2 != 0] {
                    let (record_ns, replay_ns) =
                        concurrent_record_and_replay(size, workers, preallocate);
                    let (record_name, replay_name) = if preallocate {
                        ("concurrent_record_preallocated", "concurrent_replay_preallocated")
                    } else {
                        ("concurrent_record", "concurrent_replay")
                    };
                    print_sample(size, workers, round, record_name, record_ns);
                    print_sample(size, workers, round, replay_name, replay_ns);
                }
            }
        }
    }
}
