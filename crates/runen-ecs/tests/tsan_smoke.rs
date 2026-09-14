use std::sync::{Arc, Barrier, mpsc};
use std::thread;

#[test]
fn synchronized_threads_update_disjoint_data() {
    let barrier = Arc::new(Barrier::new(2));
    let (sender, receiver) = mpsc::channel();
    let mut left = 0_u32;
    let mut right = 0_u32;

    thread::scope(|scope| {
        let left_barrier = Arc::clone(&barrier);
        let left_sender = sender.clone();
        let left_ref = &mut left;
        scope.spawn(move || {
            left_barrier.wait();
            *left_ref += 1;
            left_sender.send(*left_ref).unwrap();
        });

        let right_barrier = Arc::clone(&barrier);
        let right_sender = sender.clone();
        let right_ref = &mut right;
        scope.spawn(move || {
            right_barrier.wait();
            *right_ref += 2;
            right_sender.send(*right_ref).unwrap();
        });

        drop(sender);

        let mut values = receiver.iter().collect::<Vec<_>>();
        values.sort_unstable();
        assert_eq!(values, vec![1, 2]);
    });

    assert_eq!((left, right), (1, 2));
}
