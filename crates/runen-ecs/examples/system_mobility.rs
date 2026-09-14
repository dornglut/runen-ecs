use runen_ecs::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Copy, Clone)]
struct Update;
impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Debug, runen_ecs::Resource)]
struct Frame(u32);

fn advance(mut frame: ResMut<Frame>) {
    frame.0 += 1;
}

fn main() {
    let mut world = World::new();
    world.insert_resource(Frame(0));

    let local_ran = Rc::new(Cell::new(false));
    let captured = Rc::clone(&local_ran);

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, advance);
    runtime
        .add_systems::<Update, _, _>(&mut world, (move || captured.set(true)).on_invoker_thread());

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<Frame>().unwrap().0, 1);
    assert!(local_ran.get());

    // Normal registration means the system has proven transferable eligibility.
    // It does not promise worker execution or parallel execution. The Rc capture
    // above is genuinely thread-bound, so that system is explicitly restricted
    // to the thread that invokes the schedule.
    println!("transferable system and explicit invoker-thread system both ran");
}
