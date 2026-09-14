#[derive(runen_ecs::Resource)]
struct EscapedCommands(Option<runen_ecs::LocalCommands<'static>>);

fn escape(mut escaped: runen_ecs::ResMut<'_, EscapedCommands>, commands: runen_ecs::LocalCommands<'_>) {
    escaped.0 = Some(commands);
}

fn main() {}
