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
    /// Executes a schedule using RunenECS's deterministic parallel realization.
    ///
    /// `worker_capacity` controls only physical worker admission. It does not
    /// change schedule semantics or deterministic-profile ECS results. The
    /// serial [`Runtime::run_schedule`] path remains the independent oracle.
    pub fn run_schedule_parallel<L: ScheduleLabel>(
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

    /// Executes a schedule in parallel and invokes `on_frontier` after each
    /// semantic deferred-publication frontier has committed.
    ///
    /// The callback runs on the schedule-invoking thread. Worker capacity and
    /// physical cohort shape do not create additional callbacks.
    pub fn run_schedule_parallel_with_deferred_publication_frontier<L, F, E>(
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

        if let Err(err) = self.validate() {
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
        Commands, Component, Query, Res, ResMut, Resource, RuntimeError, ScheduleLabel,
        SystemConfigExt, SystemMobilityExt, SystemSet, World,
    };
    use std::cell::RefCell;
    use std::io;
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

    #[derive(Debug)]
    struct Sequence(Vec<i32>);
    impl Resource for Sequence {}

    #[derive(Debug)]
    struct MissingWorkerResource;
    impl Resource for MissingWorkerResource {}

    #[test]
    fn worker_resource_presence_is_checked_at_preparation_with_context() {
        fn consume(_: Res<MissingWorkerResource>) {}

        let mut world = World::new();
        let mut runtime = Runtime::new();
        runtime.add_systems(ParallelSchedule, consume).unwrap();

        let error = runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 1)
            .unwrap_err();
        match error {
            RuntimeError::Param { system, source } => {
                assert!(system.contains("consume"));
                assert!(matches!(source, crate::SystemParamError::Resource(_)));
            }
            other => panic!("expected structured worker parameter error, got {other:?}"),
        }
    }

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
        let _ = runtime.add_systems(ParallelSchedule, (first, second));
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
        let _ = runtime.add_systems(
            ParallelSchedule,
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
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            ParallelSchedule,
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
        let _ = runtime.add_systems(
            ParallelSchedule,
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
        let _ = runtime.add_systems(ParallelSchedule, (first, local, third));
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
        let _ = runtime.add_systems(ParallelSchedule, (lower_rank, higher_rank));
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
        let _ = runtime.add_systems(ParallelSchedule, (lower_rank, higher_rank));
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
            let _ = runtime.add_systems(
                ParallelSchedule,
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
    fn successful_parallel_corpus_matches_the_serial_oracle() {
        fn run(parallel: bool, capacity: usize) -> (i32, i32, i32, usize, u64, bool, bool) {
            let mut world = World::new();
            let entity = world.spawn((A(1), B(2))).unwrap();
            world.insert_resource(Counter(3));
            let before = world.current_change_cursor();
            let mut runtime = Runtime::new();
            let _ = runtime.add_systems(
                ParallelSchedule,
                (
                    move |mut query: Query<&mut A>| query.get(entity).unwrap().0 += 4,
                    move |mut query: Query<&mut B>| query.get(entity).unwrap().0 += 5,
                    |mut counter: ResMut<Counter>, mut commands: Commands| {
                        counter.0 += 6;
                        commands.spawn(DeferredMarker(9));
                    },
                ),
            );
            if parallel {
                runtime
                    .run_schedule_parallel::<ParallelSchedule>(&mut world, capacity)
                    .unwrap();
            } else {
                runtime
                    .run_schedule::<ParallelSchedule>(&mut world)
                    .unwrap();
            }
            let after = world.current_change_cursor();
            (
                world.get::<A>(entity).unwrap().0,
                world.get::<B>(entity).unwrap().0,
                world.resource::<Counter>().unwrap().0,
                world
                    .query_state::<&DeferredMarker, ()>()
                    .iter(&world)
                    .count(),
                after.tick() - before.tick(),
                world.component_changed_since::<A>(before).unwrap(),
                world.resource_changed_since::<Counter>(before).unwrap(),
            )
        }

        let serial = run(false, 1);
        assert_eq!(serial, run(true, 1));
        assert_eq!(serial, run(true, 2));
        assert_eq!(serial, run(true, 4));
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
        let _ = runtime.add_systems(ParallelSchedule, (error_system, later));
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
        let _ = runtime.add_systems(ParallelSchedule, (panic_system, later));
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
        let _ = runtime.add_systems(ParallelSchedule, system);
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

    #[test]
    fn ordinary_failures_use_reference_rank_not_completion_order() {
        let higher_finished = Arc::new(AtomicBool::new(false));
        let wait_for_higher = Arc::clone(&higher_finished);
        let lower = move || -> io::Result<()> {
            while !wait_for_higher.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            Err(io::Error::other("lower-rank error"))
        };
        let mark_higher = Arc::clone(&higher_finished);
        let higher = move || -> io::Result<()> {
            mark_higher.store(true, Ordering::Release);
            Err(io::Error::other("higher-rank error"))
        };

        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, (lower, higher));
        let error = runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut World::new(), 2)
            .expect_err("the lower-rank ordinary error must be returned");
        match error {
            RuntimeError::System { source, .. } => {
                assert_eq!(source.to_string(), "lower-rank error");
            }
            other => panic!("expected selected lower-rank system error, got {other:?}"),
        }
    }

    #[test]
    fn mixed_ordinary_error_and_user_panic_keep_ranked_selection() {
        let higher_finished = Arc::new(AtomicBool::new(false));
        let wait_for_higher = Arc::clone(&higher_finished);
        let lower_error = move || -> io::Result<()> {
            while !wait_for_higher.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            Err(io::Error::other("lower-rank error"))
        };
        let mark_higher = Arc::clone(&higher_finished);
        let higher_panic = move || -> io::Result<()> {
            mark_higher.store(true, Ordering::Release);
            panic!("higher-rank panic");
        };
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, (lower_error, higher_panic));
        let error = runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut World::new(), 2)
            .expect_err("the lower-rank ordinary error must win over a higher panic");
        assert!(matches!(error, RuntimeError::System { .. }));

        let higher_finished = Arc::new(AtomicBool::new(false));
        let wait_for_higher = Arc::clone(&higher_finished);
        let lower_panic = move || -> io::Result<()> {
            while !wait_for_higher.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            panic!("lower-rank panic");
        };
        let mark_higher = Arc::clone(&higher_finished);
        let higher_error = move || -> io::Result<()> {
            mark_higher.store(true, Ordering::Release);
            Err(io::Error::other("higher-rank error"))
        };
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, (lower_panic, higher_error));
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut World::new(), 2);
        }))
        .expect_err("the lower-rank user panic must be resumed");
        assert_eq!(
            payload.downcast_ref::<&'static str>(),
            Some(&"lower-rank panic")
        );
    }

    #[test]
    fn multiple_user_panics_select_the_lowest_ranked_payload() {
        let higher_finished = Arc::new(AtomicBool::new(false));
        let wait_for_higher = Arc::clone(&higher_finished);
        let lower = move || -> () {
            while !wait_for_higher.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            panic!("lower-rank panic");
        };
        let mark_higher = Arc::clone(&higher_finished);
        let higher = move || -> () {
            mark_higher.store(true, Ordering::Release);
            panic!("higher-rank panic");
        };
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, (lower, higher));
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut World::new(), 2);
        }))
        .expect_err("the lowest-rank user panic must be resumed");
        assert_eq!(
            payload.downcast_ref::<&'static str>(),
            Some(&"lower-rank panic")
        );
    }

    #[test]
    fn higher_rank_framework_invariants_dominate_ordinary_failures() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let scope = world.scope_id();
        world.set_change_cursor_for_test(ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1));
        let lower = || -> io::Result<()> { Err(io::Error::other("ordinary failure")) };
        let higher = move |mut query: Query<(&mut A, &mut B)>| {
            let _ = query.get(entity).unwrap();
        };
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, (lower, higher));
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 2);
        }))
        .expect_err("framework invariant must dominate an ordinary error");
        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );
    }

    #[test]
    fn higher_rank_framework_invariants_dominate_user_panics_and_are_not_text_classified() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let lower = || -> () {
            panic!("ECS change cursor exhausted");
        };
        let scope = world.scope_id();
        world.set_change_cursor_for_test(ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1));
        let higher = move |mut query: Query<(&mut A, &mut B)>| {
            let _ = query.get(entity).unwrap();
        };
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, (lower, higher));
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 2);
        }))
        .expect_err("framework invariant must dominate a same-text user panic");
        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );
    }

    #[test]
    fn rank_associated_framework_invariants_use_lowest_rank() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let higher_started = Arc::new(AtomicBool::new(false));
        let wait_for_higher = Arc::clone(&higher_started);
        let lower = move || -> () {
            while !wait_for_higher.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            crate::world::panic_worker_projection_violation("lower-rank invariant");
        };
        let mark_higher = Arc::clone(&higher_started);
        let scope = world.scope_id();
        world.set_change_cursor_for_test(ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1));
        let higher = move |mut query: Query<(&mut A, &mut B)>| {
            mark_higher.store(true, Ordering::Release);
            let _ = query.get(entity).unwrap();
        };
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, (lower, higher));
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 2);
        }))
        .expect_err("the lowest-rank framework invariant must be selected");
        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::WorkerProjectionViolation)
        );
    }

    #[test]
    fn admitted_journal_events_reconcile_on_user_panic_without_double_exhaustion() {
        let mut world = World::new();
        let entity = world.spawn(A(1)).unwrap();
        let scope = world.scope_id();
        let before = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1);
        world.set_change_cursor_for_test(before);
        let system = move |mut query: Query<&mut A>| -> () {
            let _ = query.get(entity).unwrap();
            panic!("user panic after admitted mutation");
        };
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, system);
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 1);
        }))
        .expect_err("the user panic must be resumed after reconciliation");
        assert_eq!(
            payload.downcast_ref::<&'static str>(),
            Some(&"user panic after admitted mutation")
        );
        assert_eq!(world.current_change_cursor().tick(), u64::MAX);
        assert!(world.component_changed_since::<A>(before).unwrap());
    }

    #[test]
    fn repeated_component_and_resource_observations_preserve_event_multiplicity() {
        let mut world = World::new();
        let entity = world.spawn(A(1)).unwrap();
        world.insert_resource(Counter(0));
        let before = world.current_change_cursor();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            ParallelSchedule,
            move |mut query: Query<&mut A>, mut counter: ResMut<Counter>| {
                let _ = query.get(entity).unwrap();
                let _ = query.get(entity).unwrap();
                let _ = &mut *counter;
                let _ = &mut *counter;
            },
        );
        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 1)
            .unwrap();
        assert_eq!(world.current_change_cursor().tick(), before.tick() + 4);
        assert!(world.component_changed_since::<A>(before).unwrap());
        assert!(world.resource_changed_since::<Counter>(before).unwrap());
    }

    #[test]
    fn failed_parallel_journals_preserve_repeated_event_order() {
        let mut world = World::new();
        let entity = world.spawn(A(1)).unwrap();
        let before = world.current_change_cursor();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            ParallelSchedule,
            move |mut query: Query<&mut A>| -> io::Result<()> {
                let _ = query.get(entity).unwrap();
                let _ = query.get(entity).unwrap();
                Err(io::Error::other("after two observations"))
            },
        );
        let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 1);
        assert_eq!(world.current_change_cursor().tick(), before.tick() + 2);
        assert!(world.component_changed_since::<A>(before).unwrap());
    }

    #[test]
    fn queued_command_failure_abandons_remainder_and_runtime_reuse_is_clean() {
        let mut world = World::new();
        world.insert_resource(Sequence(Vec::new()));
        let foreign = World::new().spawn(A(0)).unwrap();
        let first = Arc::new(AtomicBool::new(true));
        let run_once = Arc::clone(&first);
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, move |mut commands: Commands| {
            commands.queue(|world| {
                world.resource_mut::<Sequence>().unwrap().0.push(1);
                Ok(())
            });
            if run_once.swap(false, Ordering::AcqRel) {
                commands.despawn(foreign);
                commands.queue(|world| {
                    world.resource_mut::<Sequence>().unwrap().0.push(99);
                    Ok(())
                });
            } else {
                commands.queue(|world| {
                    world.resource_mut::<Sequence>().unwrap().0.push(2);
                    Ok(())
                });
            }
        });
        assert!(matches!(
            runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 1),
            Err(RuntimeError::Command(_))
        ));
        assert_eq!(world.resource::<Sequence>().unwrap().0, vec![1]);
        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 1)
            .unwrap();
        assert_eq!(world.resource::<Sequence>().unwrap().0, vec![1, 1, 2]);
    }

    #[test]
    fn queued_command_panic_resumes_and_runtime_reuse_is_clean() {
        let mut world = World::new();
        world.insert_resource(Sequence(Vec::new()));
        let first = Arc::new(AtomicBool::new(true));
        let run_once = Arc::clone(&first);
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(ParallelSchedule, move |mut commands: Commands| {
            commands.queue(|world| {
                world.resource_mut::<Sequence>().unwrap().0.push(1);
                Ok(())
            });
            if run_once.swap(false, Ordering::AcqRel) {
                commands.queue(|world| {
                    world.resource_mut::<Sequence>().unwrap().0.push(99);
                    panic!("queued command panic");
                });
                commands.queue(|world| {
                    world.resource_mut::<Sequence>().unwrap().0.push(100);
                    Ok(())
                });
            } else {
                commands.queue(|world| {
                    world.resource_mut::<Sequence>().unwrap().0.push(2);
                    Ok(())
                });
            }
        });
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 1);
        }))
        .expect_err("queued command panic must resume on the invoker");
        assert_eq!(
            payload.downcast_ref::<&'static str>(),
            Some(&"queued command panic")
        );
        assert_eq!(world.resource::<Sequence>().unwrap().0, vec![1, 99]);
        runtime
            .run_schedule_parallel::<ParallelSchedule>(&mut world, 1)
            .unwrap();
        assert_eq!(world.resource::<Sequence>().unwrap().0, vec![1, 99, 1, 2]);
    }

    #[test]
    fn ordinary_worker_error_and_panic_allow_clean_runtime_reuse() {
        fn run(panic_mode: bool) {
            let mut world = World::new();
            world.insert_resource(Sequence(Vec::new()));
            let first = Arc::new(AtomicBool::new(true));
            let run_once = Arc::clone(&first);
            let mut runtime = Runtime::new();
            let _ = runtime.add_systems(
                ParallelSchedule,
                move |mut sequence: ResMut<Sequence>, mut commands: Commands| -> io::Result<()> {
                    let first_run = run_once.swap(false, Ordering::AcqRel);
                    if first_run {
                        sequence.0.push(1);
                        commands.queue(|world| {
                            world.resource_mut::<Sequence>().unwrap().0.push(99);
                            Ok(())
                        });
                        if panic_mode {
                            panic!("worker failure for reuse");
                        }
                        return Err(io::Error::other("worker error for reuse"));
                    }
                    sequence.0.push(2);
                    commands.queue(|world| {
                        world.resource_mut::<Sequence>().unwrap().0.push(3);
                        Ok(())
                    });
                    Ok(())
                },
            );
            if panic_mode {
                let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 1);
                }))
                .expect_err("worker panic must resume on the invoker");
                assert_eq!(
                    payload.downcast_ref::<&'static str>(),
                    Some(&"worker failure for reuse")
                );
            } else {
                assert!(matches!(
                    runtime.run_schedule_parallel::<ParallelSchedule>(&mut world, 1),
                    Err(RuntimeError::System { .. })
                ));
            }
            assert_eq!(world.resource::<Sequence>().unwrap().0, vec![1]);
            runtime
                .run_schedule_parallel::<ParallelSchedule>(&mut world, 1)
                .unwrap();
            assert_eq!(world.resource::<Sequence>().unwrap().0, vec![1, 2, 3]);
        }

        run(false);
        run(true);
    }

    #[test]
    fn boundary_callback_failure_commits_frontier_and_stops_later_work() {
        let mut world = World::new();
        world.insert_resource(Sequence(Vec::new()));
        let later_ran = Arc::new(AtomicBool::new(false));
        let later_flag = Arc::clone(&later_ran);
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            ParallelSchedule,
            (
                (|mut commands: Commands| {
                    commands.queue(|world| {
                        world.resource_mut::<Sequence>().unwrap().0.push(1);
                        Ok(())
                    });
                })
                .in_set(ProducerSet),
                (move || later_flag.store(true, Ordering::Release)).after(ProducerSet),
            ),
        );
        let error = runtime
            .run_schedule_parallel_with_deferred_publication_frontier::<ParallelSchedule, _, _>(
                &mut world,
                2,
                |_frontier, _world| Err::<(), _>(io::Error::other("callback failed")),
            )
            .expect_err("callback error must stop the schedule");
        assert!(matches!(error, RuntimeError::Boundary { .. }));
        assert_eq!(world.resource::<Sequence>().unwrap().0, vec![1]);
        assert!(!later_ran.load(Ordering::Acquire));
    }

    #[test]
    fn boundary_callback_panic_preserves_payload_and_stops_later_work() {
        let mut world = World::new();
        world.insert_resource(Sequence(Vec::new()));
        let later_ran = Arc::new(AtomicBool::new(false));
        let later_flag = Arc::clone(&later_ran);
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            ParallelSchedule,
            (
                (|mut commands: Commands| {
                    commands.queue(|world| {
                        world.resource_mut::<Sequence>().unwrap().0.push(1);
                        Ok(())
                    });
                })
                .in_set(ProducerSet),
                (move || later_flag.store(true, Ordering::Release)).after(ProducerSet),
            ),
        );
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = runtime.run_schedule_parallel_with_deferred_publication_frontier::<
                ParallelSchedule,
                _,
                io::Error,
            >(&mut world, 2, |_frontier, _world| {
                panic!("boundary callback panic");
            });
        }))
        .expect_err("callback panic must resume on the invoker");
        assert_eq!(
            payload.downcast_ref::<&'static str>(),
            Some(&"boundary callback panic")
        );
        assert_eq!(world.resource::<Sequence>().unwrap().0, vec![1]);
        assert!(!later_ran.load(Ordering::Acquire));
    }
}
