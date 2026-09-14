use std::sync::MutexGuard;

#[derive(runen_ecs::Component)]
struct SyncButNotSend(MutexGuard<'static, ()>);

fn assert_transferable<P: runen_ecs::TransferableSystemParam>()
where
    P::State: Send,
{
}

fn main() {
    assert_transferable::<runen_ecs::Query<'static, 'static, &mut SyncButNotSend>>();
}
