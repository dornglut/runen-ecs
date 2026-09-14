#[derive(runen_ecs::SystemParam)]
struct LocalGroup<'w> {
    commands: runen_ecs::Commands<'w>,
}

fn assert_transferable<P: runen_ecs::TransferableSystemParam>()
where
    P::State: Send,
{
}

fn main() {
    assert_transferable::<LocalGroup<'static>>();
}
