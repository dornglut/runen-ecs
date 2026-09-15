use runen_ecs::LocalCommands;
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
    runtime.add_systems(Update, advance).unwrap();
    runtime
        .add_systems(
            Update,
            (move |mut commands: LocalCommands| {
                let deferred_capture = Rc::clone(&captured);
                commands.queue(move |_world| {
                    deferred_capture.set(true);
                    Ok(())
                });
            })
            .on_invoker_thread(),
        )
        .unwrap();

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<Frame>().unwrap().0, 1);
    assert!(local_ran.get());

    // Normal registration means proven transferable eligibility. LocalCommands
    // can carry !Send deferred work, so that capability is imported explicitly
    // and the system is restricted to the thread that invokes the schedule.
    println!("default transferable system and explicit local-command system both ran");
}
