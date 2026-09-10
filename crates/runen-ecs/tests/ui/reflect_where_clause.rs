use runen_ecs::Reflect;

trait Marker {}
impl Marker for u32 {}

#[derive(Reflect)]
struct WithAuthoredBounds<T>
where
    T: Marker + Reflect,
{
    value: T,
}

fn main() {
    let _ = WithAuthoredBounds::<u32>::type_info();
}
