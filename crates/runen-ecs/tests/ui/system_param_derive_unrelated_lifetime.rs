#[derive(runen_ecs::Resource)]
struct Counter;

#[derive(runen_ecs::SystemParam)]
struct Unrelated<'a> {
    value: runen_ecs::Res<'a, Counter>,
}

fn main() {}
