use runen_ecs::Reflect;

#[derive(Reflect)]
struct Borrowed<'a> {
    value: &'a str,
}

fn main() {}
