use runen_ecs::{LocalCommands, Runtime, World};

#[derive(Copy, Clone)]
struct Update;
impl runen_ecs::ScheduleLabel for Update {}

fn local_system(_: LocalCommands<'_>) {}

fn main() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, local_system);
}
