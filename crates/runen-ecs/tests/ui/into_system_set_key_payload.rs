use runen_ecs::prelude::*;

#[derive(IntoSystemSetKey)]
enum Payload {
    Unit,
    Value(u32),
}

fn main() {}
