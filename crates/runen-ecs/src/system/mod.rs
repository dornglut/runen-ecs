mod extract;
mod params;
mod runtime;

use std::fmt;

/// Semantic direction of an explicit schedule ordering declaration.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrderingDirection {
    Before,
    After,
}

impl fmt::Display for OrderingDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Before => "before",
            Self::After => "after",
        })
    }
}

/// Snapshot-local descriptor used to identify one system in schedule diagnostics.
///
/// The occurrence is descriptive only. It is not a runtime system id, execution rank,
/// or portable identity across independently built schedules.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SystemDiagnosticDescriptor {
    name: String,
    same_name_occurrence: usize,
}

impl SystemDiagnosticDescriptor {
    pub(crate) fn new(name: impl Into<String>, same_name_occurrence: usize) -> Self {
        Self {
            name: name.into(),
            same_name_occurrence,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn same_name_occurrence(&self) -> usize {
        self.same_name_occurrence
    }
}

impl fmt::Display for SystemDiagnosticDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}#{}", self.name, self.same_name_occurrence)
    }
}

/// Human-readable target-set descriptor used by schedule diagnostics.
///
/// Matching still uses the internal `SystemSetKey`; this descriptor intentionally does
/// not promote `TypeId` into portable diagnostic identity.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SystemSetDiagnosticDescriptor {
    name: &'static str,
}

impl SystemSetDiagnosticDescriptor {
    pub(crate) const fn new(name: &'static str) -> Self {
        Self { name }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }
}

impl fmt::Display for SystemSetDiagnosticDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name)
    }
}

pub use crate::scheduler::label::{ScheduleKey, ScheduleLabel, SystemSet, SystemSetKey};
pub use crate::scheduler::plan::ScheduleValidationError;
pub use crate::scheduler::system::ParamSlotDescriptor;
pub use extract::{SystemParam, SystemParamContext, SystemParamError};
pub use params::{Res, ResMut, WorldMut};
pub use runtime::{
    ConfiguredSystem, DeferredApplyBoundary, IntoSystem, IntoSystemConfigs, IntoSystemSetKey,
    Runtime, SystemConfigExt,
};
