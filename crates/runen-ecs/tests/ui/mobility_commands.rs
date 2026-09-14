fn assert_transferable<P: runen_ecs::TransferableSystemParam>()
where
    P::State: Send,
{
}

fn main() {
    assert_transferable::<runen_ecs::Commands<'static>>();
}
