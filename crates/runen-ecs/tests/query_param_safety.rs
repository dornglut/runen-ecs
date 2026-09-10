use runen_ecs::prelude::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component)]
struct Position(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Resource)]
struct Counter(i32);

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "QueryParamSafetyUpdate"
    }
}

#[test]
fn duplicate_res_mut_params_are_rejected_before_system_execution() {
    fn invalid(_first: ResMut<Counter>, _second: ResMut<Counter>) {
        panic!("conflicting system must never execute");
    }

    let mut world = World::new();
    world.insert_resource(Counter(0));
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, invalid);

    let error = runtime
        .run_schedule::<Update>(&mut world)
        .expect_err("duplicate mutable resource borrows must be rejected");
    let message = format!("{error:#}");
    assert!(message.contains("conflicting param borrows"), "{message}");
    assert!(message.contains("resource"), "{message}");
    assert!(message.contains("Counter"), "{message}");
}

#[test]
fn mutable_and_shared_resource_params_are_rejected_before_system_execution() {
    fn invalid(_writer: ResMut<Counter>, _reader: Res<Counter>) {
        panic!("conflicting system must never execute");
    }

    let mut world = World::new();
    world.insert_resource(Counter(0));
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, invalid);

    let error = runtime
        .run_schedule::<Update>(&mut world)
        .expect_err("mutable/shared resource aliases must be rejected");
    assert!(
        format!("{error:#}").contains("conflicting param borrows"),
        "{error:#}"
    );
}

#[test]
fn conflicting_query_params_are_rejected_before_system_execution() {
    fn invalid(_writer: Query<&mut Position>, _reader: Query<&Position>) {
        panic!("conflicting system must never execute");
    }

    let mut world = World::new();
    world
        .spawn(Position(1))
        .expect("fixture spawn should succeed");
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, invalid);

    let error = runtime
        .run_schedule::<Update>(&mut world)
        .expect_err("mutable/shared query aliases must be rejected");
    let message = format!("{error:#}");
    assert!(message.contains("conflicting param borrows"), "{message}");
    assert!(message.contains("component"), "{message}");
    assert!(message.contains("Position"), "{message}");
}

#[derive(runen_ecs::SystemParam)]
struct InnerBorrowGroup<'w> {
    _counter: ResMut<'w, Counter>,
}

#[derive(runen_ecs::SystemParam)]
struct NestedBorrowGroup<'w> {
    _inner: InnerBorrowGroup<'w>,
    _counter: Res<'w, Counter>,
}

#[test]
fn nested_derived_param_borrow_conflicts_are_rejected() {
    fn invalid(_group: NestedBorrowGroup<'_>) {
        panic!("conflicting system must never execute");
    }

    let mut world = World::new();
    world.insert_resource(Counter(0));
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, invalid);

    let error = runtime
        .run_schedule::<Update>(&mut world)
        .expect_err("nested derived borrow aliases must be rejected");
    assert!(
        format!("{error:#}").contains("conflicting param borrows"),
        "{error:#}"
    );
}
