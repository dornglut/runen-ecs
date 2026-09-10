use std::any::{TypeId, type_name};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ScheduleKey {
    type_id: TypeId,
    name: &'static str,
}

impl ScheduleKey {
    pub fn of<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
        }
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

pub trait ScheduleLabel: Copy + 'static {
    fn name() -> &'static str {
        type_name::<Self>()
    }

    fn key() -> ScheduleKey {
        ScheduleKey::of::<Self>(Self::name())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SystemSetKey {
    type_id: TypeId,
    name: &'static str,
}

impl SystemSetKey {
    pub fn of<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
        }
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

pub trait SystemSet: Copy + 'static {
    fn name() -> &'static str {
        type_name::<Self>()
    }

    fn key() -> SystemSetKey {
        SystemSetKey::of::<Self>(Self::name())
    }
}
