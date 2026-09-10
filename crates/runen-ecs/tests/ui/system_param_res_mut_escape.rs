#[derive(runen_ecs::Resource)]
struct Counter;

#[derive(runen_ecs::Resource)]
struct Escaped(Option<runen_ecs::ResMut<'static, Counter>>);

fn escape(mut destination: runen_ecs::ResMut<'_, Escaped>, value: runen_ecs::ResMut<'_, Counter>) {
    destination.0 = Some(value);
}

fn main() {}
