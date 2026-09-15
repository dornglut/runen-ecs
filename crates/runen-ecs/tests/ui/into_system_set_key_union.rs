use runen_ecs::prelude::*;

#[derive(IntoSystemSetKey)]
union NotAnEnum {
    value: u32,
}

fn main() {}
