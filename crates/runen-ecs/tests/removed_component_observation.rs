use runen_ecs::prelude::*;

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "RemovedQueryUpdate"
    }
}

#[derive(Copy, Clone)]
struct QueueSet;

impl SystemSet for QueueSet {
    fn name() -> &'static str {
        "RemovedQueryQueueSet"
    }
}

#[derive(Copy, Clone)]
struct ObserveSet;

impl SystemSet for ObserveSet {
    fn name() -> &'static str {
        "RemovedQueryObserveSet"
    }
}

#[derive(Copy, Clone)]
struct LateObserveSet;

impl SystemSet for LateObserveSet {
    fn name() -> &'static str {
        "RemovedQueryLateObserveSet"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct A(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct B(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Resource)]
struct Target(Entity);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Resource)]
struct TargetPair(Entity, Entity);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Resource)]
struct Gate(bool);

#[derive(Debug, Default, PartialEq, Eq, runen_ecs::Resource)]
struct StageCounts {
    same_stage: Vec<usize>,
    post_stage: Vec<usize>,
    late_stage: Vec<usize>,
}

#[derive(Debug, Default, PartialEq, Eq, runen_ecs::Resource)]
struct EntityHistory(Vec<Vec<Entity>>);

#[derive(Debug, Default, PartialEq, Eq, runen_ecs::Resource)]
struct TypeIsolationCounts {
    a_entities: Vec<Entity>,
    b_entities: Vec<Entity>,
}

#[derive(Debug, Default, PartialEq, Eq, runen_ecs::Resource)]
struct DoubleReadCounts(Vec<(usize, usize)>);

#[derive(Debug, Default, PartialEq, Eq, runen_ecs::Resource)]
struct WindowSnapshot {
    removed: Vec<usize>,
    live: Vec<usize>,
}

#[test]
fn explicit_remove_is_not_visible_before_flush_and_visible_after_flush() {
    fn queue_remove_once(mut gate: ResMut<Gate>, target: Res<Target>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.remove::<A>(target.0);
        gate.0 = true;
    }

    fn observe_same_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.same_stage.push(removed.iter().count());
    }

    fn observe_post_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.post_stage.push(removed.iter().count());
    }

    let mut world = World::new();
    let entity = world.spawn(A(1)).expect("spawn should succeed");
    world.insert_resource(Target(entity));
    world.insert_resource(Gate(false));
    world.insert_resource(StageCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, queue_remove_once.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(&mut world, observe_same_stage.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_post_stage.in_set(ObserveSet).after(QueueSet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    let counts = world.resource::<StageCounts>().unwrap();
    assert_eq!(counts.same_stage, vec![0]);
    assert_eq!(counts.post_stage, vec![1]);
}

#[test]
fn despawn_with_component_is_visible_after_flush() {
    fn queue_despawn_once(mut gate: ResMut<Gate>, target: Res<Target>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.despawn(target.0);
        gate.0 = true;
    }

    fn observe_post_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.post_stage.push(removed.iter().count());
    }

    let mut world = World::new();
    let entity = world.spawn((A(2), B(9))).expect("spawn should succeed");
    world.insert_resource(Target(entity));
    world.insert_resource(Gate(false));
    world.insert_resource(StageCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, queue_despawn_once.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_post_stage.in_set(ObserveSet).after(QueueSet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    let counts = world.resource::<StageCounts>().unwrap();
    assert_eq!(counts.post_stage, vec![1]);
}

#[test]
fn removed_records_are_visible_only_for_one_stage_window() {
    fn queue_remove_once(mut gate: ResMut<Gate>, target: Res<Target>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.remove::<A>(target.0);
        gate.0 = true;
    }

    fn observe_post_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.post_stage.push(removed.iter().count());
    }

    fn observe_late_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.late_stage.push(removed.iter().count());
    }

    let mut world = World::new();
    let entity = world.spawn(A(3)).expect("spawn should succeed");
    world.insert_resource(Target(entity));
    world.insert_resource(Gate(false));
    world.insert_resource(StageCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, queue_remove_once.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_post_stage.in_set(ObserveSet).after(QueueSet),
    );
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_late_stage.in_set(LateObserveSet).after(ObserveSet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    let counts = world.resource::<StageCounts>().unwrap();
    assert_eq!(counts.post_stage, vec![1]);
    assert_eq!(counts.late_stage, vec![0]);
}

#[test]
fn multiple_removals_in_one_stage_are_reported() {
    fn queue_remove_pair_once(
        mut gate: ResMut<Gate>,
        targets: Res<TargetPair>,
        mut commands: Commands,
    ) {
        if gate.0 {
            return;
        }
        commands.remove::<A>(targets.0);
        commands.remove::<A>(targets.1);
        gate.0 = true;
    }

    fn observe_entities(mut removed: RemovedQuery<A>, mut history: ResMut<EntityHistory>) {
        let mut entities = removed
            .iter()
            .map(|record| record.entity())
            .collect::<Vec<_>>();
        entities.sort_unstable();
        history.0.push(entities);
    }

    let mut world = World::new();
    let first = world.spawn(A(10)).expect("spawn should succeed");
    let second = world.spawn(A(11)).expect("spawn should succeed");
    world.insert_resource(TargetPair(first, second));
    world.insert_resource(Gate(false));
    world.insert_resource(EntityHistory::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, queue_remove_pair_once.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_entities.in_set(ObserveSet).after(QueueSet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(world.resource::<EntityHistory>().unwrap().0, vec![expected]);
}

#[test]
fn query_removed_is_component_type_isolated() {
    fn queue_type_specific_removes_once(
        mut gate: ResMut<Gate>,
        targets: Res<TargetPair>,
        mut commands: Commands,
    ) {
        if gate.0 {
            return;
        }
        commands.remove::<A>(targets.0);
        commands.remove::<B>(targets.1);
        gate.0 = true;
    }

    fn observe_type_isolation(
        mut removed_a: RemovedQuery<A>,
        mut removed_b: RemovedQuery<B>,
        mut counts: ResMut<TypeIsolationCounts>,
    ) {
        counts.a_entities = removed_a.iter().map(|record| record.entity()).collect();
        counts.b_entities = removed_b.iter().map(|record| record.entity()).collect();
    }

    let mut world = World::new();
    let entity_a = world.spawn((A(4), B(14))).expect("spawn should succeed");
    let entity_b = world.spawn((A(5), B(15))).expect("spawn should succeed");
    world.insert_resource(TargetPair(entity_a, entity_b));
    world.insert_resource(Gate(false));
    world.insert_resource(TypeIsolationCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        queue_type_specific_removes_once.in_set(QueueSet),
    );
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_type_isolation.in_set(ObserveSet).after(QueueSet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    let counts = world.resource::<TypeIsolationCounts>().unwrap();
    assert_eq!(counts.a_entities, vec![entity_a]);
    assert_eq!(counts.b_entities, vec![entity_b]);
}

#[test]
fn removed_entries_do_not_repeat_across_later_runs() {
    fn queue_remove_once(mut gate: ResMut<Gate>, target: Res<Target>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.remove::<A>(target.0);
        gate.0 = true;
    }

    fn observe_post_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.post_stage.push(removed.iter().count());
    }

    let mut world = World::new();
    let entity = world.spawn(A(6)).expect("spawn should succeed");
    world.insert_resource(Target(entity));
    world.insert_resource(Gate(false));
    world.insert_resource(StageCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, queue_remove_once.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_post_stage.in_set(ObserveSet).after(QueueSet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let counts = world.resource::<StageCounts>().unwrap();
    assert_eq!(counts.post_stage, vec![1, 0]);
}

#[test]
fn repeated_iter_calls_return_same_window_snapshot() {
    fn queue_remove_once(mut gate: ResMut<Gate>, target: Res<Target>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.remove::<A>(target.0);
        gate.0 = true;
    }

    fn observe_twice(mut removed: RemovedQuery<A>, mut counts: ResMut<DoubleReadCounts>) {
        let first = removed.iter().count();
        let second = removed.iter().count();
        counts.0.push((first, second));
    }

    let mut world = World::new();
    let entity = world.spawn(A(7)).expect("spawn should succeed");
    world.insert_resource(Target(entity));
    world.insert_resource(Gate(false));
    world.insert_resource(DoubleReadCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, queue_remove_once.in_set(QueueSet));
    runtime
        .add_systems::<Update, _, _>(&mut world, observe_twice.in_set(ObserveSet).after(QueueSet));

    runtime.run_schedule::<Update>(&mut world).unwrap();

    let counts = world.resource::<DoubleReadCounts>().unwrap();
    assert_eq!(counts.0, vec![(1, 1)]);
}

#[test]
fn remove_then_reinsert_in_one_flush_still_reports_removed_removal() {
    fn queue_remove_then_reinsert_once(
        mut gate: ResMut<Gate>,
        target: Res<Target>,
        mut commands: Commands,
    ) {
        if gate.0 {
            return;
        }
        commands.remove::<A>(target.0);
        commands.insert(target.0, A(99));
        gate.0 = true;
    }

    fn observe_window(
        mut removed: RemovedQuery<A>,
        mut live: Query<&A>,
        mut snapshot: ResMut<WindowSnapshot>,
    ) {
        snapshot.removed.push(removed.iter().count());
        snapshot.live.push(live.iter().count());
    }

    let mut world = World::new();
    let entity = world.spawn(A(8)).expect("spawn should succeed");
    world.insert_resource(Target(entity));
    world.insert_resource(Gate(false));
    world.insert_resource(WindowSnapshot::default());

    let mut runtime = Runtime::new();
    runtime
        .add_systems::<Update, _, _>(&mut world, queue_remove_then_reinsert_once.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_window.in_set(ObserveSet).after(QueueSet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    let snapshot = world.resource::<WindowSnapshot>().unwrap();
    assert_eq!(snapshot.removed, vec![1]);
    assert_eq!(snapshot.live, vec![1]);
    assert_eq!(world.require::<A>(entity).unwrap().0, 99);
}

#[test]
fn previous_run_removed_window_is_visible_in_next_run_first_stage_only() {
    fn queue_remove_once(mut gate: ResMut<Gate>, target: Res<Target>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.remove::<A>(target.0);
        gate.0 = true;
    }

    fn observe_same_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.same_stage.push(removed.iter().count());
    }

    let mut world = World::new();
    let entity = world.spawn(A(9)).expect("spawn should succeed");
    world.insert_resource(Target(entity));
    world.insert_resource(Gate(false));
    world.insert_resource(StageCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (
            queue_remove_once.in_set(QueueSet),
            observe_same_stage.in_set(QueueSet),
        ),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    let counts = world.resource::<StageCounts>().unwrap();
    assert_eq!(counts.same_stage, vec![0, 1, 0]);
}

#[test]
fn query_removed_does_not_conflict_with_live_mut_query_access() {
    fn mutate_live(mut query: Query<&mut A>) {
        for value in query.iter() {
            value.0 += 1;
        }
    }

    fn observe_removed(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.same_stage.push(removed.iter().count());
    }

    let mut world = World::new();
    let entity = world.spawn(A(3)).expect("spawn should succeed");
    world.insert_resource(StageCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (mutate_live, observe_removed));

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.require::<A>(entity).unwrap().0, 4);
    assert_eq!(world.resource::<StageCounts>().unwrap().same_stage, vec![0]);
}

#[test]
fn batch_remove_and_despawn_preserve_removed_stage_window_semantics() {
    fn queue_batch_once(mut gate: ResMut<Gate>, targets: Res<TargetPair>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.batch(|batch| {
            batch.remove::<A>(targets.0);
            batch.despawn(targets.1);
        });
        gate.0 = true;
    }

    fn observe_same_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.same_stage.push(removed.iter().count());
    }

    fn observe_post_stage(mut removed: RemovedQuery<A>, mut counts: ResMut<StageCounts>) {
        counts.post_stage.push(removed.iter().count());
    }

    let mut world = World::new();
    let remove_only = world.spawn((A(20), B(1))).expect("spawn should succeed");
    let despawn_target = world.spawn((A(21), B(2))).expect("spawn should succeed");
    world.insert_resource(TargetPair(remove_only, despawn_target));
    world.insert_resource(Gate(false));
    world.insert_resource(StageCounts::default());

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, queue_batch_once.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(&mut world, observe_same_stage.in_set(QueueSet));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        observe_post_stage.in_set(ObserveSet).after(QueueSet),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    let counts = world.resource::<StageCounts>().unwrap();
    assert_eq!(counts.same_stage, vec![0]);
    assert_eq!(counts.post_stage, vec![2]);
}
