use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use runen_ecs::ScheduleKey;
use runen_ecs::prelude::*;

#[derive(ScheduleLabel)]
struct Update;

struct CustomName;
impl ScheduleLabel for CustomName {
    fn name() -> &'static str {
        "Custom"
    }
}

struct CollidingScheduleA;
impl ScheduleLabel for CollidingScheduleA {
    fn name() -> &'static str {
        "SharedSchedule"
    }
}

struct CollidingScheduleB;
impl ScheduleLabel for CollidingScheduleB {
    fn name() -> &'static str {
        "SharedSchedule"
    }
}

fn hash(key: ScheduleKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn derive_uses_short_diagnostic_name_without_copy_boilerplate() {
    assert_eq!(Update::name(), "Update");

    let mut runtime = Runtime::new();
    runtime.add_systems(Update, || {}).unwrap();
    let mut world = World::new();
    runtime.run_schedule::<Update>(&mut world).unwrap();
}

#[test]
fn schedule_key_identity_is_type_only() {
    let first = ScheduleKey::of::<CustomName>("First");
    let second = ScheduleKey::of::<CustomName>("Second");

    assert_eq!(first, second);
    assert_eq!(hash(first), hash(second));
    assert_eq!(first.type_id(), second.type_id());
    assert_ne!(first.name(), second.name());
}

#[test]
fn different_schedule_types_remain_distinct_when_names_collide() {
    let first_runs = Arc::new(AtomicUsize::new(0));
    let second_runs = Arc::new(AtomicUsize::new(0));

    let first_probe = Arc::clone(&first_runs);
    let second_probe = Arc::clone(&second_runs);

    let mut runtime = Runtime::new();
    runtime
        .add_systems(CollidingScheduleA, move || {
            first_probe.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    runtime
        .add_systems(CollidingScheduleB, move || {
            second_probe.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

    let mut world = World::new();
    runtime
        .run_schedule::<CollidingScheduleA>(&mut world)
        .unwrap();
    assert_eq!(first_runs.load(Ordering::SeqCst), 1);
    assert_eq!(second_runs.load(Ordering::SeqCst), 0);

    runtime
        .run_schedule::<CollidingScheduleB>(&mut world)
        .unwrap();
    assert_eq!(first_runs.load(Ordering::SeqCst), 1);
    assert_eq!(second_runs.load(Ordering::SeqCst), 1);
}

#[test]
fn schedule_label_derive_rejects_non_unit_inputs() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/schedule_label_enum.rs");
    cases.compile_fail("tests/ui/schedule_label_named_struct.rs");
    cases.compile_fail("tests/ui/schedule_label_tuple_struct.rs");
    cases.compile_fail("tests/ui/schedule_label_union.rs");
}
