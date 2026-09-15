use runen_ecs::prelude::*;

#[derive(SystemSet)]
union NotAnEnum {
    value: u32,
}

fn main() {}
