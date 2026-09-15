use runen_ecs::prelude::*;

#[derive(Copy, Clone)]
struct Update;
impl ScheduleLabel for Update {}

#[derive(Copy, Clone, SystemSet)]
struct Prepare;

#[derive(Copy, Clone, SystemSet)]
struct Simulation;

#[derive(Copy, Clone, SystemSet)]
struct DebugOutput;

#[derive(Debug, Default, Resource)]
struct Order(Vec<&'static str>);

fn prepare(mut order: ResMut<Order>) {
    order.0.push("prepare");
}

fn simulate(mut order: ResMut<Order>) {
    order.0.push("simulate");
}

fn main() {
    let mut world = World::new();
    world.insert_resource(Order::default());

    let mut runtime = Runtime::new();
    // Register in the opposite order to make the contract visible: explicit
    // precedence, not registration order or the shared write conflict, decides.
    // `Prepare` is a present required reference. `DebugOutput` is deliberately
    // absent: optional ordering references remain valid when their target is absent.
    runtime
        .add_systems(
            Update,
            (
                simulate
                    .in_set(Simulation)
                    .after(Prepare)
                    .before_if_present(DebugOutput),
                prepare.in_set(Prepare),
            ),
        )
        .unwrap();

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(
        world.resource::<Order>().unwrap().0,
        ["prepare", "simulate"]
    );
    println!("semantic order: prepare -> simulate");
}
