use runen_ecs::prelude::*;

#[derive(Copy, Clone)]
struct Update;
impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Copy, Clone)]
struct Prepare;
impl SystemSet for Prepare {
    fn name() -> &'static str {
        "Prepare"
    }
}

#[derive(Copy, Clone)]
struct Simulation;
impl SystemSet for Simulation {
    fn name() -> &'static str {
        "Simulation"
    }
}

#[derive(Copy, Clone)]
struct DebugOutput;
impl SystemSet for DebugOutput {
    fn name() -> &'static str {
        "DebugOutput"
    }
}

#[derive(Debug, Default, runen_ecs::Resource)]
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
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (
            simulate
                .in_set(Simulation)
                .after(Prepare)
                .before_if_present(DebugOutput),
            prepare.in_set(Prepare),
        ),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(
        world.resource::<Order>().unwrap().0,
        ["prepare", "simulate"]
    );
    println!("semantic order: prepare -> simulate");
}
