use runen_ecs::Reflect;

#[derive(Reflect)]
union Unsupported {
    value: u32,
}

fn main() {}
