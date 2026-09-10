use runen_ecs::Reflect;

#[derive(Reflect)]
struct Generic<T>
where
    T: Reflect,
{
    value: T,
}

fn main() {
    let _ = Generic::<u32>::type_info();
    let _ = Generic::<String>::type_info();
}
