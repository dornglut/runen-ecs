use runen_ecs::{CommandError, Commands, World};
use std::rc::Rc;

fn main() {
    let mut commands = Commands::new();
    let local = Rc::new(());
    commands.queue(move |_: &mut World| {
        let _ = Rc::strong_count(&local);
        Ok::<(), CommandError>(())
    });
}
