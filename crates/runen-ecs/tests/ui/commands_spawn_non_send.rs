use runen_ecs::{Component, Commands};
use std::rc::Rc;

#[derive(Component)]
struct LocalComponent(Rc<()>);

fn main() {
    let mut commands = Commands::new();
    commands.spawn(LocalComponent(Rc::new(())));
}
