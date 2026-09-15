use std::any::{TypeId, type_name};
use std::hash::{Hash, Hasher};

#[derive(Debug, Copy, Clone)]
pub struct ScheduleKey {
    type_id: TypeId,
    name: &'static str,
    diagnostic_type_name: &'static str,
}

impl ScheduleKey {
    pub fn of<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
            diagnostic_type_name: type_name::<T>(),
        }
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) const fn diagnostic_type_name(&self) -> &'static str {
        self.diagnostic_type_name
    }
}

impl PartialEq for ScheduleKey {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.name == other.name
    }
}

impl Eq for ScheduleKey {}

impl Hash for ScheduleKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.name.hash(state);
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

#[derive(Debug, Copy, Clone)]
pub struct SystemSetKey {
    type_id: TypeId,
    name: &'static str,
    diagnostic_type_name: &'static str,
}

impl SystemSetKey {
    pub fn of<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
            diagnostic_type_name: type_name::<T>(),
        }
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) const fn diagnostic_type_name(&self) -> &'static str {
        self.diagnostic_type_name
    }
}

impl PartialEq for SystemSetKey {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.name == other.name
    }
}

impl Eq for SystemSetKey {}

impl Hash for SystemSetKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.name.hash(state);
    }
}

/// A value that identifies a set used for system membership and ordering.
///
/// Ordinary unit markers use type-based identity by default. A derived fieldless
/// enum gives each variant a distinct set identity while retaining the enum's
/// [`TypeId`](std::any::TypeId), with names such as `CoreSet::Simulation`.
/// Names are diagnostic and scheduler set-key data; they are not persistence or
/// network identity.
///
/// ```
/// use runen_ecs::prelude::*;
///
/// #[derive(Copy, Clone, SystemSet)]
/// enum CoreSet {
///     Input,
///     Simulation,
/// }
///
/// let set = CoreSet::Simulation;
/// assert_eq!(set.key().name(), "CoreSet::Simulation");
/// ```
pub trait SystemSet: Copy + 'static {
    fn name(&self) -> &'static str {
        type_name::<Self>()
    }

    fn key(&self) -> SystemSetKey {
        SystemSetKey::of::<Self>(self.name())
    }
}
