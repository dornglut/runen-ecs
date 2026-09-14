use std::cell::Cell;

#[derive(runen_ecs::Component)]
struct SendButNotSync(Cell<u32>);

fn assert_transferable<P: runen_ecs::TransferableSystemParam>()
where
    P::State: Send,
{
}

fn main() {
    assert_transferable::<runen_ecs::Query<'static, 'static, &SendButNotSync>>();
}
