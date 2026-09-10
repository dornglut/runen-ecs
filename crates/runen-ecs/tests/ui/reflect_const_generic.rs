use runen_ecs::Reflect;

struct ConstField<const N: usize>([u8; N]);

impl<const N: usize> Reflect for ConstField<N> {
    fn type_info() -> runen_ecs::TypeInfo {
        runen_ecs::TypeInfo::new(
            std::any::type_name::<Self>(),
            "ConstField",
            runen_ecs::ReflectShape::Opaque,
        )
    }
}

#[derive(Reflect)]
struct ConstGeneric<const N: usize> {
    value: ConstField<N>,
}

fn main() {
    let _ = ConstGeneric::<4>::type_info();
    let _ = ConstGeneric::<8>::type_info();
}
