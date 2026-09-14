use super::{DeferredCommandsUnwindGuard, DeferredPublicationFrontier, InvocationOutcome, Runtime};
use crate::World;
use crate::commands::TransferableCommandBuffer;
use crate::errors::RuntimeError;
use crate::scheduler::label::ScheduleLabel;
use crate::scheduler::plan::ExecutionPlan;
use crate::scheduler::system::RegisteredSystem;
use crate::system::ExecutionMobility;
use crate::system::worker_cohort::run_worker_cohort;
use std::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParallelStep {
    Invoker {
        reference_rank: usize,
        system_index: usize,
    },
    Workers {
        members: Vec<(usize, usize)>,
    },
}

impl Runtime {
    /// Internal baseline deterministic-parallel executor used by #39 conformance.
    ///
    /// The public serial executor remains the independent semantic oracle. #40
    /// owns final supported selector/failure-permutation acceptance.
    pub(crate) fn run_schedule_parallel<L: ScheduleLabel>(
        &mut self,
        world: &mut World,
        worker_capacity: usize,
    ) -> Result<(), RuntimeError> {
        self.run_schedule_parallel_with_deferred_publication_frontier::<L, _, _>(
            world,
            worker_capacity,
            |_frontier, _world| Ok::<(), RuntimeError>(()),
        )
    }

    pub(crate) fn run_schedule_parallel_with_deferred_publication_frontier<L, F, E>(
        &mut self,
        world: &mut World,
        worker_capacity: usize,
        mut on_frontier: F,
    ) -> Result<(), RuntimeError>
    where
        L: ScheduleLabel,
        F: FnMut(DeferredPublicationFrontier, &mut World) -> Result<(), E>,
        E: Into<Box<dyn Error + Send + Sync>> + 'static,
    {
        let _unwind_guard = DeferredCommandsUnwindGuard::new(self.deferred_commands.clone());

        if worker_capacity == 0 {
            self.discard_deferred_commands();
            return Err(RuntimeError::Setup {
                message: "parallel worker capacity must be greater than zero".to_string(),
            });
        }

        if let Err(err) = self.ensure_build_ready() {
            self.discard_deferred_commands();
            return Err(err);
        }

        let plan = match self.scheduler.plan_for::<L>() {
            Ok(Some(plan)) => plan.clone(),
            Ok(None) => return Ok(()),
            Err(err) => {
                self.discard_deferred_commands();
                return Err(err.into());
            }
        };

        let mut next_reference_rank = 0usize;
        let mut next_frontier = 0usize;
        while next_reference_rank < plan.reference_system_indices.len() {
            let step = match plan_parallel_step(
                &plan,
                self.scheduler.systems(),
                next_reference_rank,
                worker_capacity,
            ) {
                Ok(step) => step,
                Err(err) => {
                    self.discard_deferred_commands();
                    return Err(err);
                }
            };

            match step {
                ParallelStep::Invoker {
                    reference_rank,
                    system_index,
                } => {
                    debug_assert_eq!(reference_rank, next_reference_rank);
                    let outcome = {
                        let Some(system) = self.scheduler.systems_mut().get_mut(system_index)
                        else {
                            self.discard_deferred_commands();
                            return Err(RuntimeError::Invariant {
                                message: "execution plan referenced missing system",
                            });
                        };
                        system.run(world)
                    };
                    match outcome {
                        Ok(outcome) => self.append_invocation_outcome(outcome),
                        Err(err) => {
                            self.discard_deferred_commands();
                            return Err(err);
                        }
                    }
                    next_reference_rank = reference_rank.saturating_add(1);
                }
                ParallelStep::Workers { members } => {
                    let next_rank = members
                        .last()
                        .map(|(rank, _)| rank.saturating_add(1))
                        .expect("planned worker step is non-empty");
                    let buffers = match run_planned_worker_step(
                        self.scheduler.systems_mut(),
                        world,
                        &members,
                    ) {
                        Ok(buffers) => buffers,
                        Err(err) => {
                            self.discard_deferred_commands();
                            return Err(err);
                        }
                    };
                    for (_rank, buffer) in buffers {
                        if let Some(buffer) = buffer {
                            self.append_invocation_outcome(InvocationOutcome::Transferable(buffer));
                        }
                    }
                    next_reference_rank = next_rank;
                }
            }

            while plan
                .publication_frontiers
                .get(next_frontier)
                .is_some_and(|frontier| frontier.cut == next_reference_rank)
            {
                if let Err(err) = self.publish_deferred_commands(world) {
                    self.discard_deferred_commands();
                    return Err(err);
                }
                if let Err(err) = on_frontier(
                    DeferredPublicationFrontier {
                        schedule: plan.label,
                        ordinal: next_frontier,
                    },
                    world,
                ) {
                    self.discard_deferred_commands();
                    return Err(RuntimeError::Boundary { source: err.into() });
                }
                next_frontier = next_frontier.saturating_add(1);
            }
        }

        if next_frontier != plan.publication_frontiers.len() {
            self.discard_deferred_commands();
            return Err(RuntimeError::Invariant {
                message: "parallel executor left a semantic publication frontier unreached",
            });
        }
        Ok(())
    }
}

fn plan_parallel_step(
    plan: &ExecutionPlan,
    systems: &[RegisteredSystem],
    start_rank: usize,
    worker_capacity: usize,
) -> Result<ParallelStep, RuntimeError> {
    let Some(&first_index) = plan.reference_system_indices.get(start_rank) else {
        return Err(RuntimeError::Invariant {
            message: "parallel executor reference rank is outside the execution plan",
        });
    };
    let Some(first) = systems.get(first_index) else {
        return Err(RuntimeError::Invariant {
            message: "parallel executor plan referenced missing system",
        });
    };
    if !system_ready_at_cut(plan, first_index, start_rank)? {
        return Err(RuntimeError::Invariant {
            message: "parallel executor reference sequence contains a system that is not ready",
        });
    }

    if first.execution_mobility() == ExecutionMobility::InvokerThreadOnly {
        return Ok(ParallelStep::Invoker {
            reference_rank: start_rank,
            system_index: first_index,
        });
    }
    if !first.worker_capable() {
        return Err(RuntimeError::Invariant {
            message: "transferable system lacks worker projection preparation proof",
        });
    }

    let next_frontier_cut = plan
        .publication_frontiers
        .iter()
        .map(|frontier| frontier.cut)
        .find(|cut| *cut > start_rank)
        .unwrap_or(plan.reference_system_indices.len());

    let mut members = vec![(start_rank, first_index)];
    let upper_bound = next_frontier_cut.min(plan.reference_system_indices.len());
    for candidate_rank in start_rank.saturating_add(1)..upper_bound {
        if members.len() >= worker_capacity {
            break;
        }
        let candidate_index = plan.reference_system_indices[candidate_rank];
        let Some(candidate) = systems.get(candidate_index) else {
            return Err(RuntimeError::Invariant {
                message: "parallel executor plan referenced missing system",
            });
        };
        if candidate.execution_mobility() == ExecutionMobility::InvokerThreadOnly {
            break;
        }
        if !candidate.worker_capable() {
            return Err(RuntimeError::Invariant {
                message: "transferable system lacks worker projection preparation proof",
            });
        }
        if !system_ready_at_cut(plan, candidate_index, start_rank)? {
            break;
        }
        if members.iter().any(|(_, selected_index)| {
            !systems[*selected_index]
                .access()
                .conflicts_with(candidate.access())
                .is_empty()
        }) {
            break;
        }
        members.push((candidate_rank, candidate_index));
    }

    Ok(ParallelStep::Workers { members })
}

fn system_ready_at_cut(
    plan: &ExecutionPlan,
    system_index: usize,
    completed_cut: usize,
) -> Result<bool, RuntimeError> {
    for reason in &plan.precedence_reasons {
        if reason.successor_system_index != system_index {
            continue;
        }
        let predecessor_rank = plan
            .reference_rank_by_system_index
            .get(reason.predecessor_system_index)
            .copied()
            .flatten()
            .ok_or(RuntimeError::Invariant {
                message: "precedence predecessor is missing from reference-rank map",
            })?;
        if predecessor_rank >= completed_cut {
            return Ok(false);
        }
    }
    Ok(true)
}

fn run_planned_worker_step(
    systems: &mut [RegisteredSystem],
    world: &mut World,
    members: &[(usize, usize)],
) -> Result<Vec<(usize, Option<TransferableCommandBuffer>)>, RuntimeError> {
    let mut runners = Vec::with_capacity(members.len());
    for (system_index, system) in systems.iter_mut().enumerate() {
        let Some((rank, _)) = members
            .iter()
            .find(|(_, candidate_index)| *candidate_index == system_index)
        else {
            continue;
        };
        let Some(runner) = system.transferable_runner_mut() else {
            return Err(RuntimeError::Invariant {
                message: "planned worker member lost its transferable runner",
            });
        };
        runners.push((*rank, runner));
    }
    if runners.len() != members.len() {
        return Err(RuntimeError::Invariant {
            message: "parallel worker step could not resolve every planned system",
        });
    }
    run_worker_cohort(world, runners)
}

#[cfg(test)]
mod tests {
    use super::{ParallelStep, plan_parallel_step};
    use crate::system::runtime::Runtime;
    use crate::world::{ChangeCursor, FrameworkInvariantKind, framework_invariant_kind};
    use crate::{
        Commands, Component, Query, ResMut, Resource, RuntimeError, ScheduleLabel, SystemConfigExt,
        SystemMobilityExt, SystemSet, World,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier, Mutex};

    #[derive(Copy, Clone)]
    struct ParallelSchedule;
    impl ScheduleLabel for ParallelSchedule {}

    #[derive(Copy, Clone)]
    struct ProducerSet;
    impl SystemSet for ProducerSet {}

    #[derive(Debug)]
    struct A(i32);
    impl Component for A {}

    #[derive(Debug)]
    struct B(i32);
    impl Component for B {}

    #[derive(Debug)]
    struct Value(i32);
    impl Component for Value {}

    #[derive(Debug)]
    struct DeferredMarker(i32);
    impl Component for DeferredMarker {}

    #[derive(Debug)]
    struct Counter(i32);
    impl Resource for Counter {}

    #[derive(Debug)]
    struct Seen(usize);
    impl Resource for Seen {}

    #[test]
    fn production_parallel_path_overlaps_compatible_workers_and_is_reusable() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(10))).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let first_barrier = Arc::clone(&barrier);
        let first = move |mut query: Query<&mut A>| {
            first_barrier.wait();
            query.get(entity).unwrap().0 += 1;
        };
        let second_barrier = Arc::clone(&barrier);
        let second = move |mut query: Query<&mut B>| {
            second_barrier.wait();
            query.get(entity).unwrap().0 += 10;
        };

        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(&mut world, (first, second));
        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 2)
            .unwrap();
        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 2)
            .unwrap();

        assert_eq!(world.get::<A>(entity).unwrap().0, 3);
        assert_eq!(world.get::<B>(entity).unwrap().0, 30);
    }

    #[test]
    fn unordered_conflicting_systems_serialize_in_reference_order() {
        let mut world = World::new();
        world.insert_resource(Counter(1));
        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(
            &mut world,
            (
                |mut counter: ResMut<Counter>| counter.0 += 1,
                |mut counter: ResMut<Counter>| counter.0 *= 10,
            ),
        );

        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 2)
            .unwrap();
        assert_eq!(world.resource::<Counter>().unwrap().0, 20);
    }

    #[test]
    fn explicit_precedence_blocks_same_launch_even_without_deferred_frontier() {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(
            &mut world,
            ((|| {}).in_set(ProducerSet), (|| {}).after(ProducerSet)),
        );
        let plan = runtime
            .scheduler
            .plan_for::<ParallelSchedule>()
            .unwrap()
            .unwrap()
            .clone();
        assert!(plan.publication_frontiers.is_empty());

        let step = plan_parallel_step(&plan, runtime.scheduler.systems(), 0, 4).unwrap();
        assert_eq!(
            step,
            ParallelStep::Workers {
                members: vec![(0, plan.reference_system_indices[0])]
            }
        );
    }

    #[test]
    fn deferred_visibility_and_callback_follow_semantic_frontier() {
        let mut world = World::new();
        world.insert_resource(Seen(0));
        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(
            &mut world,
            (
                (|mut commands: Commands| commands.spawn(DeferredMarker(7))).in_set(ProducerSet),
                (|mut query: Query<&DeferredMarker>, mut seen: ResMut<Seen>| {
                    seen.0 = query.iter().count();
                })
                .after(ProducerSet),
            ),
        );

        let mut frontier_ordinals = Vec::new();
        runtime
            .run_schedule_parallel_with_deferred_publication_frontier::<ParallelSchedule, _, _>(
                &mut world,
                2,
                |frontier, _world| {
                    frontier_ordinals.push(frontier.ordinal());
                    Ok::<(), RuntimeError>(())
                },
            )
            .unwrap();

        assert_eq!(world.resource::<Seen>().unwrap().0, 1);
        assert_eq!(frontier_ordinals, vec![0]);
        let marker = world
            .query_state::<&DeferredMarker, ()>()
            .single(&world)
            .unwrap();
        assert_eq!(marker.0, 7);
    }

    #[test]
    fn invoker_thread_only_system_is_a_physical_fence() {
        let mut world = World::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let caller = std::thread::current().id();
        let observed_thread = Rc::new(RefCell::new(None));

        let first_log = Arc::clone(&log);
        let first = move || first_log.lock().unwrap().push(1u8);
        let local_log = Arc::clone(&log);
        let local_thread = Rc::clone(&observed_thread);
        let local = (move || {
            local_log.lock().unwrap().push(2u8);
            *local_thread.borrow_mut() = Some(std::thread::current().id());
        })
        .on_invoker_thread();
        let third_log = Arc::clone(&log);
        let third = move || third_log.lock().unwrap().push(3u8);

        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(&mut world, (first, local, third));
        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 3)
            .unwrap();

        assert_eq!(*log.lock().unwrap(), vec![1, 2, 3]);
        assert_eq!(observed_thread.borrow().as_ref(), Some(&caller));
    }

    #[test]
    fn worker_commands_publish_in_reference_order_not_completion_order() {
        let mut world = World::new();
        let entity = world.spawn(Value(0)).unwrap();
        let higher_finished = Arc::new(AtomicBool::new(false));

        let wait_for_higher = Arc::clone(&higher_finished);
        let lower_rank = move |mut commands: Commands| {
            while !wait_for_higher.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            commands.insert(entity, Value(1));
        };
        let mark_higher = Arc::clone(&higher_finished);
        let higher_rank = move |mut commands: Commands| {
            commands.insert(entity, Value(2));
            mark_higher.store(true, Ordering::Release);
        };

        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(&mut world, (lower_rank, higher_rank));
        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 2)
            .unwrap();
        assert_eq!(world.get::<Value>(entity).unwrap().0, 2);
    }

    #[test]
    fn public_change_order_uses_reference_rank_not_reservation_timing() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let higher_reserved = Arc::new(AtomicBool::new(false));

        let wait_for_higher = Arc::clone(&higher_reserved);
        let lower_rank = move |mut query: Query<&mut A>| {
            while !wait_for_higher.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            query.get(entity).unwrap().0 += 1;
        };
        let mark_higher = Arc::clone(&higher_reserved);
        let higher_rank = move |mut query: Query<&mut B>| {
            query.get(entity).unwrap().0 += 1;
            mark_higher.store(true, Ordering::Release);
        };

        let before = world.current_change_cursor();
        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(&mut world, (lower_rank, higher_rank));
        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 2)
            .unwrap();

        let a_tick = world.archetype_component_metadata::<A>(entity).unwrap().1;
        let b_tick = world.archetype_component_metadata::<B>(entity).unwrap().1;
        assert!(a_tick < b_tick);
        assert_eq!(world.current_change_cursor().tick(), before.tick() + 2);
    }

    #[test]
    fn worker_capacity_does_not_change_successful_ecs_results() {
        fn run(capacity: usize) -> (i32, i32, usize, u64, u64, bool, bool) {
            let mut world = World::new();
            let entity = world.spawn((A(1), B(2))).unwrap();
            let before = world.current_change_cursor();
            let mut runtime = Runtime::new();
            runtime.add_systems::<ParallelSchedule, _, _>(
                &mut world,
                (
                    move |mut query: Query<&mut A>| query.get(entity).unwrap().0 += 3,
                    move |mut query: Query<&mut B>| query.get(entity).unwrap().0 += 5,
                    |mut commands: Commands| commands.spawn(DeferredMarker(9)),
                ),
            );
            runtime
                .run_schedule_parallel::<ParallelSchedule>(&mut world, capacity)
                .unwrap();
            let marker_count = world
                .query_state::<&DeferredMarker, ()>()
                .iter(&world)
                .count();
            let after = world.current_change_cursor();
            (
                world.get::<A>(entity).unwrap().0,
                world.get::<B>(entity).unwrap().0,
                marker_count,
                after.epoch(),
                after.tick(),
                world.component_changed_since::<A>(before).unwrap(),
                world.component_changed_since::<B>(before).unwrap(),
            )
        }

        assert_eq!(run(1), run(3));
    }

    #[test]
    fn ordinary_worker_error_stops_before_later_invoker_step() {
        let mut world = World::new();
        let later_ran = Rc::new(std::cell::Cell::new(false));
        let later_flag = Rc::clone(&later_ran);
        let error_system =
            || -> std::io::Result<()> { Err(std::io::Error::other("worker failure")) };
        let later = (move || later_flag.set(true)).on_invoker_thread();

        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(&mut world, (error_system, later));
        let result = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 2);
        assert!(matches!(result, Err(RuntimeError::System { .. })));
        assert!(!later_ran.get());
    }

    #[test]
    fn user_panic_resumes_on_invoker_and_stops_later_work() {
        fn panic_system() {
            panic!("worker user panic");
        }

        let mut world = World::new();
        let later_ran = Rc::new(std::cell::Cell::new(false));
        let later_flag = Rc::clone(&later_ran);
        let later = (move || later_flag.set(true)).on_invoker_thread();

        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(&mut world, (panic_system, later));
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 2);
        }))
        .expect_err("worker user panic must resume on invoking thread");
        assert_eq!(
            payload.downcast_ref::<&'static str>(),
            Some(&"worker user panic")
        );
        assert!(!later_ran.get());
    }

    #[test]
    fn cursor_exhaustion_remains_framework_invariant_on_production_path() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let body_ran = Arc::new(AtomicBool::new(false));
        let body_flag = Arc::clone(&body_ran);
        let system = move |mut query: Query<(&mut A, &mut B)>| {
            let _ = query.get(entity).unwrap();
            body_flag.store(true, Ordering::Relaxed);
        };
        let scope = world.scope_id();
        let before = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1);
        world.set_change_cursor_for_test(before);

        let mut runtime = Runtime::new();
        runtime.add_systems::<ParallelSchedule, _, _>(&mut world, system);
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 1);
        }))
        .expect_err("cursor exhaustion must remain a framework panic");

        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );
        assert!(!body_ran.load(Ordering::Relaxed));
        assert!(world.component_changed_since::<A>(before).unwrap());
        assert!(!world.component_changed_since::<B>(before).unwrap());
    }
}
