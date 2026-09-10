#[derive(runen_ecs::Component)]
struct Position;

#[derive(runen_ecs::Resource)]
struct Escaped(Option<runen_ecs::Query<'static, 'static, &'static mut Position>>);

fn escape(
    mut destination: runen_ecs::ResMut<'_, Escaped>,
    query: runen_ecs::Query<'_, '_, &'static mut Position>,
) {
    destination.0 = Some(query);
}

fn main() {}
