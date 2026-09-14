use runen_ecs::{Component, TransferableCommands};
use std::rc::Rc;

#[derive(Component)]
struct LocalComponent(Rc<()>);

fn main() {
    let mut commands = TransferableCommands::new();
    commands.spawn(LocalComponent(Rc::new(())));
}
