use runen_ecs::prelude::*;

#[derive(SystemSet)]
enum Payload {
    Unit,
    Value(u32),
}

fn main() {}
