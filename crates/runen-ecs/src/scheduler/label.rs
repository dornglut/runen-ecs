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

    pub(crate) fn for_label<L: ScheduleLabel>() -> Self {
        Self::of::<L>(L::name())
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
        self.type_id == other.type_id
    }
}

impl Eq for ScheduleKey {}

impl Hash for ScheduleKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
    }
}

/// A type-level label that selects one ECS schedule.
///
/// Schedule identity is exactly the Rust type. `name()` is diagnostic text and
/// never participates in scheduler identity. Ordinary labels should use
/// `#[derive(ScheduleLabel)]`; manual implementations are useful when a custom
/// diagnostic name is intentional.
pub trait ScheduleLabel: 'static {
    fn name() -> &'static str {
        type_name::<Self>()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct SystemSetKey {
    type_id: TypeId,
    discriminator: u64,
    name: &'static str,
    diagnostic_type_name: &'static str,
}

impl SystemSetKey {
    pub fn of<T: 'static>(name: &'static str) -> Self {
        Self::with_discriminator::<T>(0, name)
    }

    #[doc(hidden)]
    pub fn with_discriminator<T: 'static>(discriminator: u64, name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            discriminator,
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
        self.type_id == other.type_id && self.discriminator == other.discriminator
    }
}

impl Eq for SystemSetKey {}

impl Hash for SystemSetKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.discriminator.hash(state);
    }
}

/// A value that identifies a set used for system membership and ordering.
///
/// Ordinary unit markers use type-based identity by default. A derived fieldless
/// enum gives each variant a distinct opaque runtime-local discriminator while
/// retaining the enum's [`TypeId`](std::any::TypeId), with diagnostic names
/// such as `CoreSet::Simulation`. Diagnostic names never participate in
/// scheduler set identity and are not persistence or network identity.
///
/// ```
/// use runen_ecs::prelude::*;
///
/// #[derive(SystemSet)]
/// enum CoreSet {
///     Input,
///     Simulation,
/// }
///
/// let set = CoreSet::Simulation;
/// assert_eq!(set.key().name(), "CoreSet::Simulation");
/// ```
pub trait SystemSet: Sized + 'static {
    fn name(&self) -> &'static str {
        type_name::<Self>()
    }

    fn key(&self) -> SystemSetKey {
        SystemSetKey::of::<Self>(self.name())
    }
}
