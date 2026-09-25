use crate::World;
use crate::commands::TransferableCommandBuffer;
use crate::errors::RuntimeError;
use crate::scheduler::system::{ControlledWorkerSystem, RegisteredSystem};
use crate::system::worker_cohort::run_worker_cohort;

/// Framework-owned concurrency harness for the #38 World-projection proof.
///
/// The supplied order is the harness's reference-rank order. This is not a
/// public scheduler: it consumes an explicitly supplied set, validates pairwise
/// compatibility before projection preparation, joins every launched worker,
/// reconciles journals canonically, and returns successful transferable command
/// buffers without publishing them.
#[allow(dead_code)]
pub(crate) fn run_controlled_worker_harness(
    world: &mut World,
    systems: Vec<RegisteredSystem>,
) -> Result<Vec<Option<TransferableCommandBuffer>>, RuntimeError> {
    let mut systems = systems
        .into_iter()
        .map(RegisteredSystem::into_controlled_worker)
        .collect::<Result<Vec<_>, _>>()?;

    validate_pairwise_compatibility(&systems)?;
    let members = systems
        .iter_mut()
        .enumerate()
        .map(|(rank, system)| (rank, system.runner_mut()))
        .collect::<Vec<_>>();
    run_worker_cohort(world, members)
        .map(|buffers| buffers.into_iter().map(|(_rank, buffer)| buffer).collect())
}

fn validate_pairwise_compatibility(systems: &[ControlledWorkerSystem]) -> Result<(), RuntimeError> {
    for left_index in 0..systems.len() {
        for right_index in (left_index + 1)..systems.len() {
            let left = &systems[left_index];
            let right = &systems[right_index];
            if let Some(conflict) = left
                .access()
                .conflicts_with(right.access())
                .into_iter()
                .next()
            {
                return Err(RuntimeError::Setup {
                    message: format!(
                        "controlled worker harness rejected '{}' and '{}': {}",
                        left.name(),
                        right.name(),
                        conflict.diagnostic_message(),
                    ),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run_controlled_worker_harness;
    use crate::scheduler::system::RegisteredSystem;
    use crate::system::IntoSystem;
    use crate::world::{ChangeCursor, FrameworkInvariantKind, framework_invariant_kind};
    use crate::{
        Added, Changed, Commands, Component, CyclePolicy, Directed, LocalCommands, Query, Relation,
        RelationConstraints, RelationsMut, RemovedQuery, Res, ResMut, Resource, ScheduleLabel,
        SourceCardinality, SystemMobilityExt, With, Without, World,
    };
    use std::cell::Cell;
    use std::marker::PhantomData;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    #[derive(Copy, Clone)]
    struct HarnessSchedule;
    impl ScheduleLabel for HarnessSchedule {}

    #[derive(Debug)]
    struct A(i32);
    impl Component for A {}

    #[derive(Debug)]
    struct B(i32);
    impl Component for B {}

    #[derive(Debug)]
    struct C(i32);
    impl Component for C {}

    #[derive(Debug)]
    struct Marker(i32);
    impl Component for Marker {}

    type FilteredMixedPointData = (&'static mut A, &'static B);
    type FilteredMixedPointFilter = (With<C>, Without<Marker>);

    #[allow(dead_code)]
    struct ThreadBound(Rc<Cell<u32>>);
    impl Component for ThreadBound {}

    #[allow(dead_code)]
    struct AbsentThreadBound(Rc<Cell<u32>>);
    impl Component for AbsentThreadBound {}

    #[allow(dead_code)]
    struct RemovedThreadBound(Rc<Cell<u32>>);
    impl Component for RemovedThreadBound {}

    #[allow(dead_code)]
    struct ThreadBoundResource(Rc<Cell<u32>>);
    impl Resource for ThreadBoundResource {}

    struct SharedOnlyComponent(PhantomData<Rc<()>>);
    // Safety: this zero-sized test witness owns no Rc; PhantomData keeps it !Send,
    // while shared access is safe across threads and deliberately proves Sync-only access.
    unsafe impl Sync for SharedOnlyComponent {}
    impl Component for SharedOnlyComponent {}

    struct MutableOnlyComponent(Cell<u32>);
    impl Component for MutableOnlyComponent {}

    struct SharedOnlyResource(PhantomData<Rc<()>>);
    // Safety: same Sync + !Send witness as SharedOnlyComponent, with no runtime payload.
    unsafe impl Sync for SharedOnlyResource {}
    impl Resource for SharedOnlyResource {}

    struct MutableOnlyResource(Cell<u32>);
    impl Resource for MutableOnlyResource {}

    #[derive(Debug)]
    struct Counter(i32);
    impl Resource for Counter {}

    #[derive(Debug)]
    struct DeferredMarker(i32);
    impl Component for DeferredMarker {}

    struct RelationA;
    impl Relation for RelationA {
        type Kind = Directed;
    }

    struct RelationB;
    impl Relation for RelationB {
        type Kind = Directed;
    }

    struct ConstrainedRelationA;
    impl Relation for ConstrainedRelationA {
        type Kind = Directed;
        const CONSTRAINTS: RelationConstraints = RelationConstraints::new()
            .source_cardinality(SourceCardinality::One)
            .cycles(CyclePolicy::Forbid);
    }

    struct ConstrainedRelationB;
    impl Relation for ConstrainedRelationB {
        type Kind = Directed;
        const CONSTRAINTS: RelationConstraints = RelationConstraints::new()
            .source_cardinality(SourceCardinality::One)
            .cycles(CyclePolicy::Forbid);
    }

    #[allow(dead_code)]
    struct ThreadBoundRelation(PhantomData<Rc<()>>);
    impl Relation for ThreadBoundRelation {
        type Kind = Directed;
    }

    macro_rules! define_relation_markers {
        ($($name:ident),+ $(,)?) => {$(
            struct $name;
            impl Relation for $name {
                type Kind = Directed;
            }
        )+};
    }

    define_relation_markers!(
        Relation0, Relation1, Relation2, Relation3, Relation4, Relation5, Relation6, Relation7,
        Relation8, Relation9, Relation10, Relation11,
    );

    #[derive(crate::SystemParam)]
    struct DerivedWorkerGroup<'w, 's> {
        query: Query<'w, 's, &'static B>,
        counter: ResMut<'w, Counter>,
        commands: Commands<'w>,
    }

    fn register<S, MarkerT>(_world: &mut World, system: S) -> RegisteredSystem
    where
        S: IntoSystem<MarkerT>,
    {
        system
            .into_registered_system::<HarnessSchedule>()
            .expect("test system should register")
    }

    #[test]
    fn controlled_worker_harness_safety_boundary() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        world.insert_resource(Counter(5));
        world.insert_resource(ThreadBoundResource(Rc::new(Cell::new(9))));
        let _ = world
            .spawn(ThreadBound(Rc::new(Cell::new(3))))
            .expect("thread-bound component should remain legal in World");

        let barrier = Arc::new(Barrier::new(2));
        let first_barrier = Arc::clone(&barrier);
        let first = register(&mut world, move |mut query: Query<&mut A>| {
            first_barrier.wait();
            query.get(entity).expect("A should exist").0 += 10;
        });
        let second_barrier = Arc::clone(&barrier);
        let second = register(&mut world, move |mut counter: ResMut<Counter>| {
            second_barrier.wait();
            counter.0 += 20;
        });

        let before = world.current_change_cursor();
        let buffers = run_controlled_worker_harness(&mut world, vec![first, second]).unwrap();
        assert!(buffers.iter().all(Option::is_none));
        assert_eq!(world.get::<A>(entity).unwrap().0, 11);
        assert_eq!(world.resource::<Counter>().unwrap().0, 25);
        assert_eq!(world.current_change_cursor().tick(), before.tick() + 2);
        assert!(world.component_changed_since::<A>(before).unwrap());
        assert!(world.resource_changed_since::<Counter>(before).unwrap());
    }

    #[test]
    fn worker_projection_uses_exact_shared_and_mutable_payload_bounds() {
        let mut world = World::new();
        let entity = world
            .spawn((
                SharedOnlyComponent(PhantomData),
                MutableOnlyComponent(Cell::new(1)),
            ))
            .unwrap();

        world.insert_resource(SharedOnlyResource(PhantomData));
        world.insert_resource(MutableOnlyResource(Cell::new(2)));

        let shared = register(
            &mut world,
            |mut query: Query<&SharedOnlyComponent>, resource: Res<SharedOnlyResource>| {
                assert_eq!(query.iter().count(), 1);
                let _ = &*resource;
            },
        );
        let mutable = register(
            &mut world,
            move |mut query: Query<&mut MutableOnlyComponent>,
                  resource: ResMut<MutableOnlyResource>| {
                query
                    .get(entity)
                    .expect("mutable-only component should exist")
                    .0
                    .set(7);
                resource.0.set(8);
            },
        );

        run_controlled_worker_harness(&mut world, vec![shared, mutable]).unwrap();
        assert_eq!(
            world.get::<MutableOnlyComponent>(entity).unwrap().0.get(),
            7
        );
        assert_eq!(world.resource::<MutableOnlyResource>().unwrap().0.get(), 8);
    }

    #[test]
    fn compatible_shared_projections_can_overlap_with_unrelated_thread_bound_payloads() {
        let mut world = World::new();
        world.spawn(A(7)).unwrap();
        world.insert_resource(ThreadBoundResource(Rc::new(Cell::new(1))));

        let barrier = Arc::new(Barrier::new(2));
        let observed = Arc::new(AtomicUsize::new(0));
        let make_system = |barrier: Arc<Barrier>, observed: Arc<AtomicUsize>| {
            move |mut query: Query<&A>| {
                barrier.wait();
                let total = query.iter().map(|value| value.0 as usize).sum::<usize>();
                observed.fetch_add(total, Ordering::Relaxed);
            }
        };
        let first = register(
            &mut world,
            make_system(Arc::clone(&barrier), Arc::clone(&observed)),
        );
        let second = register(
            &mut world,
            make_system(Arc::clone(&barrier), Arc::clone(&observed)),
        );

        run_controlled_worker_harness(&mut world, vec![first, second]).unwrap();
        assert_eq!(observed.load(Ordering::Relaxed), 14);
    }

    #[test]
    fn conflicting_access_is_rejected_before_any_worker_body_runs() {
        let mut world = World::new();
        world.spawn(A(1)).unwrap();
        let first_ran = Arc::new(AtomicBool::new(false));
        let second_ran = Arc::new(AtomicBool::new(false));
        let first_flag = Arc::clone(&first_ran);
        let first = register(&mut world, move |_query: Query<&mut A>| {
            first_flag.store(true, Ordering::Relaxed);
        });
        let second_flag = Arc::clone(&second_ran);
        let second = register(&mut world, move |_query: Query<&mut A>| {
            second_flag.store(true, Ordering::Relaxed);
        });

        let result = run_controlled_worker_harness(&mut world, vec![first, second]);
        assert!(matches!(result, Err(crate::RuntimeError::Setup { .. })));
        assert!(!first_ran.load(Ordering::Relaxed));
        assert!(!second_ran.load(Ordering::Relaxed));
    }

    #[test]
    fn worker_query_preserves_iter_get_and_single() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2), C(3))).unwrap();
        let system = register(
            &mut world,
            move |mut a: Query<&mut A>, mut b: Query<&B>, mut c: Query<&C>| {
                assert_eq!(a.iter().count(), 1);
                assert_eq!(b.get(entity).expect("B should exist").0, 2);
                assert_eq!(c.single().expect("C should be unique").0, 3);
            },
        );

        run_controlled_worker_harness(&mut world, vec![system]).unwrap();
    }

    type ThreadBoundObservation = (With<ThreadBound>, Changed<ThreadBound>, Added<ThreadBound>);

    #[test]
    fn metadata_only_worker_queries_do_not_inherit_thread_bound_payload_traits() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Marker(7),
                ThreadBound(Rc::new(Cell::new(1))),
                RemovedThreadBound(Rc::new(Cell::new(2))),
            ))
            .unwrap();
        let _ = world
            .remove::<RemovedThreadBound>(entity)
            .expect("removed component should exist");
        world.insert_resource(ThreadBoundResource(Rc::new(Cell::new(3))));

        let combined_ran = Arc::new(AtomicBool::new(false));
        let combined_flag = Arc::clone(&combined_ran);
        let combined = register(
            &mut world,
            move |mut query: Query<&Marker, ThreadBoundObservation>| {
                let values = query.iter().map(|marker| marker.0).collect::<Vec<_>>();
                assert_eq!(values, vec![7]);
                combined_flag.store(true, Ordering::Relaxed);
            },
        );

        let without_ran = Arc::new(AtomicBool::new(false));
        let without_flag = Arc::clone(&without_ran);
        let without = register(
            &mut world,
            move |mut query: Query<&Marker, Without<AbsentThreadBound>>| {
                assert_eq!(query.iter().count(), 1);
                without_flag.store(true, Ordering::Relaxed);
            },
        );

        let removed_ran = Arc::new(AtomicBool::new(false));
        let removed_flag = Arc::clone(&removed_ran);
        let removed = register(
            &mut world,
            move |mut removed: RemovedQuery<RemovedThreadBound>| {
                assert_eq!(removed.iter().count(), 1);
                removed_flag.store(true, Ordering::Relaxed);
            },
        );

        run_controlled_worker_harness(&mut world, vec![combined, without, removed]).unwrap();
        assert!(combined_ran.load(Ordering::Relaxed));
        assert!(without_ran.load(Ordering::Relaxed));
        assert!(removed_ran.load(Ordering::Relaxed));
    }

    #[test]
    fn derived_system_param_keeps_type_directed_worker_preparation() {
        let mut world = World::new();
        world.spawn(B(4)).unwrap();
        world.insert_resource(Counter(1));
        let system = register(&mut world, |mut group: DerivedWorkerGroup| {
            assert_eq!(group.query.iter().count(), 1);
            group.counter.0 += 2;
            group.commands.spawn(DeferredMarker(11));
        });

        let buffers = run_controlled_worker_harness(&mut world, vec![system]).unwrap();
        assert_eq!(world.resource::<Counter>().unwrap().0, 3);
        assert_eq!(world.query::<&DeferredMarker>().iter(&world).count(), 0);
        assert_eq!(buffers.len(), 1);
        for buffer in buffers.into_iter().flatten() {
            buffer.apply(&mut world).unwrap();
        }
        let value = world
            .query::<&DeferredMarker>()
            .single(&world)
            .expect("derived worker Commands buffer should publish on the invoker");
        assert_eq!(value.0, 11);
    }

    #[test]
    fn commands_are_worker_local_and_only_finalized_buffer_crosses_back() {
        let mut world = World::new();
        let system = register(&mut world, |mut commands: Commands| {
            commands.spawn(DeferredMarker(9));
        });

        let buffers = run_controlled_worker_harness(&mut world, vec![system]).unwrap();
        assert_eq!(world.query::<&DeferredMarker>().iter(&world).count(), 0);
        assert_eq!(buffers.len(), 1);
        for buffer in buffers.into_iter().flatten() {
            buffer.apply(&mut world).unwrap();
        }
        let value = world
            .query::<&DeferredMarker>()
            .single(&world)
            .expect("published worker buffer should spawn the component");
        assert_eq!(value.0, 9);
    }

    #[test]
    fn invoker_thread_only_system_is_excluded_before_execution() {
        let mut world = World::new();
        let ran = Rc::new(Cell::new(false));
        let ran_for_system = Rc::clone(&ran);
        let system = register(
            &mut world,
            (move |_commands: LocalCommands| {
                ran_for_system.set(true);
            })
            .on_invoker_thread(),
        );

        let result = run_controlled_worker_harness(&mut world, vec![system]);
        assert!(matches!(result, Err(crate::RuntimeError::Setup { .. })));
        assert!(!ran.get());
    }

    #[test]
    fn worker_query_point_locator_preserves_filtered_mixed_lookup_semantics() {
        let mut world = World::new();
        let early = world.spawn((A(1), B(10), C(100))).unwrap();
        let middle = world.spawn((A(2), B(20), C(200))).unwrap();
        let late = world.spawn((A(3), B(30), C(300))).unwrap();
        let filtered_out = world.spawn((A(4), B(40), C(400), Marker(1))).unwrap();
        let non_member = world.spawn((B(50), C(500))).unwrap();
        let stale = world.spawn((A(6), B(60), C(600))).unwrap();
        world.despawn(stale).unwrap();

        let mut foreign_world = World::new();
        let foreign = foreign_world.spawn((A(7), B(70), C(700))).unwrap();

        let system = register(
            &mut world,
            move |mut query: Query<FilteredMixedPointData, FilteredMixedPointFilter>| {
                for (entity, expected_a, expected_b) in
                    [(early, 1, 10), (middle, 2, 20), (late, 3, 30)]
                {
                    let (a, b) = query.get(entity).expect("matching entity should resolve");
                    assert_eq!(a.0, expected_a);
                    assert_eq!(b.0, expected_b);
                    a.0 += b.0;
                }

                assert!(query.get(filtered_out).is_none());
                assert!(query.get(non_member).is_none());
                assert!(query.get(stale).is_none());
                assert!(query.get(foreign).is_none());
            },
        );

        let before = world.current_change_cursor();
        run_controlled_worker_harness(&mut world, vec![system]).unwrap();

        assert_eq!(world.get::<A>(early).unwrap().0, 11);
        assert_eq!(world.get::<A>(middle).unwrap().0, 22);
        assert_eq!(world.get::<A>(late).unwrap().0, 33);
        assert_eq!(world.get::<A>(filtered_out).unwrap().0, 4);
        assert_eq!(world.current_change_cursor().tick(), before.tick() + 3);
        assert!(world.component_changed_since::<A>(before).unwrap());
    }

    #[test]
    fn multi_mutable_query_preserves_event_multiplicity_and_local_order() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let system = register(&mut world, move |mut query: Query<(&mut A, &mut B)>| {
            let (a, b) = query.get(entity).expect("tuple should exist");
            a.0 += 10;
            b.0 += 20;
        });
        let before = world.current_change_cursor();

        run_controlled_worker_harness(&mut world, vec![system]).unwrap();
        assert_eq!(world.current_change_cursor().tick(), before.tick() + 2);
        assert!(world.component_changed_since::<A>(before).unwrap());
        assert!(world.component_changed_since::<B>(before).unwrap());
    }

    #[test]
    fn repeated_same_domain_events_preserve_multiplicity_and_local_order() {
        let mut world = World::new();
        let entity = world.spawn(A(1)).unwrap();
        world.insert_resource(Counter(0));
        let barrier = Arc::new(Barrier::new(2));

        let query_barrier = Arc::clone(&barrier);
        let query_system = register(&mut world, move |mut query: Query<&mut A>| {
            query_barrier.wait();
            query.get(entity).expect("A should exist").0 += 1;
            query.get(entity).expect("A should still exist").0 += 1;
        });

        let resource_barrier = Arc::clone(&barrier);
        let resource_system = register(&mut world, move |mut counter: ResMut<Counter>| {
            resource_barrier.wait();
            counter.0 += 1;
        });

        let before = world.current_change_cursor();
        run_controlled_worker_harness(&mut world, vec![query_system, resource_system]).unwrap();

        let a_changed = world.archetype_component_metadata::<A>(entity).unwrap().1;
        assert_eq!(world.get::<A>(entity).unwrap().0, 3);
        assert_eq!(world.resource::<Counter>().unwrap().0, 1);
        assert_eq!(a_changed.epoch(), before.epoch());
        assert_eq!(a_changed.tick(), before.tick() + 2);
        assert_eq!(world.current_change_cursor().epoch(), before.epoch());
        assert_eq!(world.current_change_cursor().tick(), before.tick() + 3);
    }

    #[test]
    fn physical_reservation_order_does_not_choose_public_change_order() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let higher_rank_reserved = Arc::new(AtomicBool::new(false));

        let wait_for_higher_rank = Arc::clone(&higher_rank_reserved);
        let lower_rank = register(&mut world, move |mut query: Query<&mut A>| {
            while !wait_for_higher_rank.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            let _ = query.get(entity).expect("A should exist");
        });

        let mark_reserved = Arc::clone(&higher_rank_reserved);
        let higher_rank = register(&mut world, move |mut query: Query<&mut B>| {
            let _ = query.get(entity).expect("B should exist");
            mark_reserved.store(true, Ordering::Release);
        });

        run_controlled_worker_harness(&mut world, vec![lower_rank, higher_rank]).unwrap();
        let a_tick = world.archetype_component_metadata::<A>(entity).unwrap().1;
        let b_tick = world.archetype_component_metadata::<B>(entity).unwrap().1;
        assert!(a_tick < b_tick);
    }

    #[test]
    fn user_panic_reconciles_all_launched_journal_prefixes() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        fn trigger_user_panic() {
            panic!("user panic");
        }

        let first = register(&mut world, move |mut query: Query<&mut A>| {
            query.get(entity).expect("A should exist").0 += 1;
            trigger_user_panic();
        });
        let second = register(&mut world, move |mut query: Query<&mut B>| {
            query.get(entity).expect("B should exist").0 += 1;
        });
        let before = world.current_change_cursor();

        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = run_controlled_worker_harness(&mut world, vec![first, second]);
        }))
        .expect_err("user panic must resume on the invoker");
        assert!(framework_invariant_kind(payload.as_ref()).is_none());
        assert!(world.component_changed_since::<A>(before).unwrap());
        assert!(world.component_changed_since::<B>(before).unwrap());
        assert_eq!(world.get::<A>(entity).unwrap().0, 2);
        assert_eq!(world.get::<B>(entity).unwrap().0, 3);
    }

    #[test]
    fn cursor_exhaustion_reconciles_admitted_prefix_before_resuming_invariant() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let body_ran = Arc::new(AtomicBool::new(false));
        let body_flag = Arc::clone(&body_ran);
        let system = register(&mut world, move |mut query: Query<(&mut A, &mut B)>| {
            let _ = query.get(entity).expect("tuple should exist");
            body_flag.store(true, Ordering::Relaxed);
        });

        let scope = world.scope_id();
        let before = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1);
        world.set_change_cursor_for_test(before);
        let exhausted = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX);

        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = run_controlled_worker_harness(&mut world, vec![system]);
        }))
        .expect_err("second event must exhaust cohort capacity");
        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );
        assert!(!body_ran.load(Ordering::Relaxed));
        assert_eq!(world.current_change_cursor(), exhausted);
        assert!(world.component_changed_since::<A>(before).unwrap());
        assert!(!world.component_changed_since::<B>(before).unwrap());
    }

    #[test]
    fn cursor_invariant_dominates_lower_rank_ordinary_system_error() {
        let mut world = World::new();
        let entity = world.spawn((A(1), B(2))).unwrap();
        let error_system = register(&mut world, || -> std::io::Result<()> {
            Err(std::io::Error::other("ordinary user error"))
        });
        let invariant_system = register(&mut world, move |mut query: Query<(&mut A, &mut B)>| {
            let _ = query.get(entity).expect("tuple should exist");
        });

        let scope = world.scope_id();
        let before = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1);
        world.set_change_cursor_for_test(before);

        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = run_controlled_worker_harness(&mut world, vec![error_system, invariant_system]);
        }))
        .expect_err("framework invariant must dominate ordinary system error");
        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );
        assert!(world.component_changed_since::<A>(before).unwrap());
        assert!(!world.component_changed_since::<B>(before).unwrap());
    }
    #[test]
    fn distinct_relation_writers_use_disjoint_worker_projections() {
        let mut world = World::new();
        let source = world.spawn(Marker(1)).unwrap();
        let target = world.spawn(Marker(2)).unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let first_barrier = Arc::clone(&barrier);
        let first = register(&mut world, move |mut relations: RelationsMut<RelationA>| {
            first_barrier.wait();
            assert!(relations.insert(source, target).unwrap());
        });

        let second_barrier = Arc::clone(&barrier);
        let second = register(&mut world, move |mut relations: RelationsMut<RelationB>| {
            second_barrier.wait();
            assert!(relations.insert(source, target).unwrap());
        });

        run_controlled_worker_harness(&mut world, vec![first, second]).unwrap();
        assert!(world.relations::<RelationA>().contains(source, target));
        assert!(world.relations::<RelationB>().contains(source, target));
    }

    #[test]
    fn distinct_constrained_relation_writers_use_disjoint_worker_projections() {
        let mut world = World::new();
        let source = world.spawn(Marker(1)).unwrap();
        let target = world.spawn(Marker(2)).unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let first_barrier = Arc::clone(&barrier);
        let first = register(
            &mut world,
            move |mut relations: RelationsMut<ConstrainedRelationA>| {
                first_barrier.wait();
                assert!(relations.insert(source, target).unwrap());
            },
        );

        let second_barrier = Arc::clone(&barrier);
        let second = register(
            &mut world,
            move |mut relations: RelationsMut<ConstrainedRelationB>| {
                second_barrier.wait();
                assert!(relations.insert(source, target).unwrap());
            },
        );

        run_controlled_worker_harness(&mut world, vec![first, second]).unwrap();
        assert!(
            world
                .relations::<ConstrainedRelationA>()
                .contains(source, target)
        );
        assert!(
            world
                .relations::<ConstrainedRelationB>()
                .contains(source, target)
        );
    }

    #[test]
    fn same_relation_writer_conflict_is_rejected_before_worker_execution() {
        let mut world = World::new();
        let first_ran = Arc::new(AtomicBool::new(false));
        let second_ran = Arc::new(AtomicBool::new(false));

        let first_flag = Arc::clone(&first_ran);
        let first = register(&mut world, move |_relations: RelationsMut<RelationA>| {
            first_flag.store(true, Ordering::Relaxed);
        });
        let second_flag = Arc::clone(&second_ran);
        let second = register(&mut world, move |_relations: RelationsMut<RelationA>| {
            second_flag.store(true, Ordering::Relaxed);
        });

        let result = run_controlled_worker_harness(&mut world, vec![first, second]);
        assert!(matches!(result, Err(crate::RuntimeError::Setup { .. })));
        assert!(!first_ran.load(Ordering::Relaxed));
        assert!(!second_ran.load(Ordering::Relaxed));
    }

    #[test]
    fn relation_worker_projection_preserves_exact_entity_validation_precedence() {
        let mut world = World::new();

        let stale = world.spawn(Marker(1)).unwrap();
        world.despawn(stale).unwrap();
        let valid = world.spawn(Marker(2)).unwrap();

        let freed = world.spawn(Marker(3)).unwrap();
        world.despawn(freed).unwrap();

        let mut foreign_world = World::new();
        let foreign = foreign_world.spawn(Marker(4)).unwrap();

        let system = register(&mut world, move |mut relations: RelationsMut<RelationA>| {
            assert!(matches!(
                relations.insert(foreign, valid),
                Err(crate::RelationError::Entity(crate::EntityError::ForeignWorld { entity }))
                    if entity == foreign
            ));
            assert!(matches!(
                relations.insert(stale, valid),
                Err(crate::RelationError::Entity(crate::EntityError::StaleGeneration {
                    entity,
                    ..
                })) if entity == stale
            ));
            assert!(matches!(
                relations.insert(freed, foreign),
                Err(crate::RelationError::Entity(crate::EntityError::AlreadyFreed { entity }))
                    if entity == freed
            ));
            assert!(relations.is_empty());
        });

        run_controlled_worker_harness(&mut world, vec![system]).unwrap();
        assert!(world.relations::<RelationA>().is_empty());
    }

    #[test]
    fn relation_store_registry_growth_keeps_all_prepared_write_projections_stable() {
        let mut world = World::new();
        let source = world.spawn(Marker(1)).unwrap();
        let target = world.spawn(Marker(2)).unwrap();

        let system = register(
            &mut world,
            move |mut r0: RelationsMut<Relation0>,
                  mut r1: RelationsMut<Relation1>,
                  mut r2: RelationsMut<Relation2>,
                  mut r3: RelationsMut<Relation3>,
                  mut r4: RelationsMut<Relation4>,
                  mut r5: RelationsMut<Relation5>,
                  mut r6: RelationsMut<Relation6>,
                  mut r7: RelationsMut<Relation7>,
                  mut r8: RelationsMut<Relation8>,
                  mut r9: RelationsMut<Relation9>,
                  mut r10: RelationsMut<Relation10>,
                  mut r11: RelationsMut<Relation11>| {
                assert!(r0.insert(source, target).unwrap());
                assert!(r1.insert(source, target).unwrap());
                assert!(r2.insert(source, target).unwrap());
                assert!(r3.insert(source, target).unwrap());
                assert!(r4.insert(source, target).unwrap());
                assert!(r5.insert(source, target).unwrap());
                assert!(r6.insert(source, target).unwrap());
                assert!(r7.insert(source, target).unwrap());
                assert!(r8.insert(source, target).unwrap());
                assert!(r9.insert(source, target).unwrap());
                assert!(r10.insert(source, target).unwrap());
                assert!(r11.insert(source, target).unwrap());
            },
        );

        run_controlled_worker_harness(&mut world, vec![system]).unwrap();
        assert!(world.relations::<Relation0>().contains(source, target));
        assert!(world.relations::<Relation5>().contains(source, target));
        assert!(world.relations::<Relation11>().contains(source, target));
    }

    #[test]
    fn relation_marker_traits_do_not_leak_into_transferability() {
        let mut world = World::new();
        let source = world.spawn(Marker(1)).unwrap();
        let target = world.spawn(Marker(2)).unwrap();

        let system = register(
            &mut world,
            move |mut relations: RelationsMut<ThreadBoundRelation>| {
                assert!(relations.insert(source, target).unwrap());
            },
        );

        run_controlled_worker_harness(&mut world, vec![system]).unwrap();
        assert!(
            world
                .relations::<ThreadBoundRelation>()
                .contains(source, target)
        );
    }
}
