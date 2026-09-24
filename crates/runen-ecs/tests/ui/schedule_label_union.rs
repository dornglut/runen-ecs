use runen_ecs::prelude::*;

#[derive(ScheduleLabel)]
union NotASchedule {
    value: u32,
}

fn main() {}
