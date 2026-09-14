mod concurrent;
mod extract;
mod params;
pub(crate) mod runtime;
mod worker_cohort;

use std::fmt;

/// The proven execution capability of a registered system.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ExecutionMobility {
    Transferable,
    InvokerThreadOnly,
}

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

/// Snapshot-local descriptor used to identify one schedule in diagnostics.
///
/// Rust type text and the occurrence discriminator are explanatory only. Neither is
/// executable schedule identity or portable identity across independently built snapshots.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScheduleDiagnosticDescriptor {
    name: &'static str,
    type_name: &'static str,
    same_name_and_type_occurrence: usize,
}

impl ScheduleDiagnosticDescriptor {
    pub(crate) const fn new(
        name: &'static str,
        type_name: &'static str,
        same_name_and_type_occurrence: usize,
    ) -> Self {
        Self {
            name,
            type_name,
            same_name_and_type_occurrence,
        }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn type_name(self) -> &'static str {
        self.type_name
    }

    pub const fn same_name_and_type_occurrence(self) -> usize {
        self.same_name_and_type_occurrence
    }
}

impl fmt::Display for ScheduleDiagnosticDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_named_descriptor(
            formatter,
            self.name,
            self.type_name,
            self.same_name_and_type_occurrence,
        )
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

/// Snapshot-local target-set descriptor used by schedule diagnostics.
///
/// Matching still uses the internal `SystemSetKey`. Rust type text and the occurrence
/// discriminator are explanatory only and never become executable or portable set identity.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SystemSetDiagnosticDescriptor {
    name: &'static str,
    type_name: &'static str,
    same_name_and_type_occurrence: usize,
}

impl SystemSetDiagnosticDescriptor {
    pub(crate) const fn new(
        name: &'static str,
        type_name: &'static str,
        same_name_and_type_occurrence: usize,
    ) -> Self {
        Self {
            name,
            type_name,
            same_name_and_type_occurrence,
        }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn type_name(self) -> &'static str {
        self.type_name
    }

    pub const fn same_name_and_type_occurrence(self) -> usize {
        self.same_name_and_type_occurrence
    }
}

impl fmt::Display for SystemSetDiagnosticDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_named_descriptor(
            formatter,
            self.name,
            self.type_name,
            self.same_name_and_type_occurrence,
        )
    }
}

fn fmt_named_descriptor(
    formatter: &mut fmt::Formatter<'_>,
    name: &'static str,
    type_name: &'static str,
    occurrence: usize,
) -> fmt::Result {
    match (name == type_name, occurrence) {
        (true, 1) => formatter.write_str(name),
        (true, occurrence) => write!(formatter, "{name}#{occurrence}"),
        (false, 1) => write!(formatter, "{name} [{type_name}]"),
        (false, occurrence) => write!(formatter, "{name} [{type_name}]#{occurrence}"),
    }
}

pub use crate::scheduler::inspection::{
    ScheduleAccessAmbiguity, ScheduleAccessConflict, ScheduleAccessConflictKind,
    ScheduleAccessDomain, ScheduleInspection, ScheduleOrderingCycle, ScheduleOrderingResolution,
    ScheduleOrderingResolutionKind, SchedulePairwiseConcurrencyAssessment, SchedulePrecedenceEdge,
    SchedulePrecedencePath, SchedulePrecedenceReason, SchedulePublicationFrontier,
    SchedulePublicationObligation,
};
pub use crate::scheduler::label::{ScheduleKey, ScheduleLabel, SystemSet, SystemSetKey};
pub use crate::scheduler::plan::ScheduleValidationError;
pub use crate::scheduler::system::OrderingPresence;
pub use crate::scheduler::system::ParamSlotDescriptor;
pub use extract::{
    DeferredRecorderClass, DeferredRecorderConflict, SystemParam, SystemParamContext,
    SystemParamError, TransferableSystemParam, WorkerPrepareContext,
};
pub use params::{Res, ResMut, WorldMut};
pub use runtime::{
    ConfiguredSystem, DeferredPublicationFrontier, IntoSystem, IntoSystemConfigs, IntoSystemSetKey,
    InvokerThreadSystem, Runtime, SystemConfigExt, SystemMobilityExt,
};
