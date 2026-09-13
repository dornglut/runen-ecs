use crate::scheduler::label::{ScheduleKey, ScheduleLabel, SystemSetKey};
use crate::scheduler::system::{OrderingDeclaration, OrderingPresence, RegisteredSystem, SystemId};
use crate::system::{OrderingDirection, SystemDiagnosticDescriptor, SystemSetDiagnosticDescriptor};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BTreeSet;
use thiserror::Error;

/// One semantic ordering layer in an ECS schedule.
///
/// Stages are formed only from explicit before/after set constraints. Access
/// incompatibilities are recorded separately and never create stage boundaries.
#[derive(Debug, Clone)]
pub(crate) struct ExecutionStage {
    pub(crate) system_indices: Vec<usize>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OrderingResolutionKind {
    Resolved {
        target_system_indices: Vec<usize>,
        target_systems: Vec<SystemDiagnosticDescriptor>,
    },
    AbsentOptional,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OrderingResolution {
    pub(crate) source_system_index: usize,
    pub(crate) source: SystemDiagnosticDescriptor,
    pub(crate) declaration: OrderingDeclaration,
    pub(crate) kind: OrderingResolutionKind,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrecedenceReason {
    pub(crate) source: SystemDiagnosticDescriptor,
    pub(crate) direction: OrderingDirection,
    pub(crate) target_set: SystemSetDiagnosticDescriptor,
    pub(crate) target_set_key: SystemSetKey,
    pub(crate) presence: OrderingPresence,
    pub(crate) predecessor_system_index: usize,
    pub(crate) predecessor: SystemDiagnosticDescriptor,
    pub(crate) successor_system_index: usize,
    pub(crate) successor: SystemDiagnosticDescriptor,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionPlan {
    pub(crate) label: ScheduleKey,
    pub(crate) stages: Vec<ExecutionStage>,
    #[allow(dead_code)]
    pub(crate) ordering_resolutions: Vec<OrderingResolution>,
    #[allow(dead_code)]
    pub(crate) precedence_reasons: Vec<PrecedenceReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleValidationError {
    #[error(
        "schedule '{schedule}' system '{source_system}' has unresolved required {direction} reference to set '{target_set}'"
    )]
    UnresolvedOrderingReference {
        schedule: &'static str,
        source_system: SystemDiagnosticDescriptor,
        direction: OrderingDirection,
        target_set: SystemSetDiagnosticDescriptor,
    },
    #[error("schedule '{schedule}' has cyclic system ordering constraints")]
    OrderingCycle { schedule: &'static str },
    #[error("schedule system identity space is exhausted")]
    SystemIdentityExhausted,
}

/// ECS-owned registry for deterministic schedule planning and serial execution.
///
/// This is deliberately World-specific. It is not a generic scheduler framework.
pub(crate) struct ScheduleRegistry {
    systems: Vec<RegisteredSystem>,
    plans: Vec<ExecutionPlan>,
    dirty: bool,
    next_system_id: Option<std::num::NonZeroU64>,
}

impl Default for ScheduleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ScheduleRegistry {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
            plans: Vec::new(),
            dirty: true,
            next_system_id: std::num::NonZeroU64::new(1),
        }
    }

    pub fn add_system(
        &mut self,
        mut system: RegisteredSystem,
    ) -> Result<usize, ScheduleValidationError> {
        let system_id = self
            .next_system_id
            .ok_or(ScheduleValidationError::SystemIdentityExhausted)?;
        self.next_system_id = system_id
            .get()
            .checked_add(1)
            .and_then(std::num::NonZeroU64::new);
        let system_id = SystemId::new(system_id);
        system.assign_id(system_id);
        let index = self.systems.len();
        self.systems.push(system);
        self.dirty = true;
        Ok(index)
    }

    pub fn systems_mut(&mut self) -> &mut [RegisteredSystem] {
        &mut self.systems
    }

    pub(crate) fn plan_for<L: ScheduleLabel>(
        &mut self,
    ) -> Result<Option<&ExecutionPlan>, ScheduleValidationError> {
        self.rebuild_if_dirty()?;
        Ok(self.plans.iter().find(|plan| plan.label == L::key()))
    }

    fn rebuild_if_dirty(&mut self) -> Result<(), ScheduleValidationError> {
        if !self.dirty {
            return Ok(());
        }
        let mut labels = Vec::<ScheduleKey>::new();
        for system in &self.systems {
            if !labels.iter().any(|label| *label == system.label()) {
                labels.push(system.label());
            }
        }
        self.plans = labels
            .into_iter()
            .map(|label| self.build_plan(label))
            .collect::<Result<Vec<_>, _>>()?;
        self.dirty = false;
        Ok(())
    }

    fn build_plan(&self, label: ScheduleKey) -> Result<ExecutionPlan, ScheduleValidationError> {
        let scheduled_indices = self
            .systems
            .iter()
            .enumerate()
            .filter_map(|(index, system)| (system.label() == label).then_some(index))
            .collect::<Vec<_>>();
        let descriptors = self.system_descriptors(&scheduled_indices);

        let mut ordering_resolutions = Vec::new();
        let mut precedence_reasons = Vec::new();
        let mut unresolved = Vec::new();

        for (source_pos, source_index) in scheduled_indices.iter().copied().enumerate() {
            let source = &self.systems[source_index];
            let mut declarations = source.ordering_declarations().to_vec();
            declarations.sort_by(compare_declarations);

            for declaration in declarations {
                let targets = scheduled_indices
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|(target_pos, target_index)| {
                        *target_pos != source_pos
                            && self.systems[*target_index]
                                .sets()
                                .iter()
                                .any(|set| *set == declaration.target())
                    })
                    .collect::<Vec<_>>();

                if targets.is_empty() {
                    if declaration.presence().is_required() {
                        unresolved.push(UnresolvedOrderingReference {
                            source: descriptors[source_pos].clone(),
                            declaration,
                        });
                    } else {
                        ordering_resolutions.push(OrderingResolution {
                            source_system_index: source_index,
                            source: descriptors[source_pos].clone(),
                            declaration,
                            kind: OrderingResolutionKind::AbsentOptional,
                        });
                    }
                    continue;
                }

                ordering_resolutions.push(OrderingResolution {
                    source_system_index: source_index,
                    source: descriptors[source_pos].clone(),
                    declaration,
                    kind: OrderingResolutionKind::Resolved {
                        target_system_indices: targets
                            .iter()
                            .map(|(_, target_index)| *target_index)
                            .collect(),
                        target_systems: targets
                            .iter()
                            .map(|(target_pos, _)| descriptors[*target_pos].clone())
                            .collect(),
                    },
                });

                for (target_pos, target_index) in targets {
                    let (predecessor_system_index, predecessor, successor_system_index, successor) =
                        match declaration.direction() {
                            OrderingDirection::Before => (
                                source_index,
                                descriptors[source_pos].clone(),
                                target_index,
                                descriptors[target_pos].clone(),
                            ),
                            OrderingDirection::After => (
                                target_index,
                                descriptors[target_pos].clone(),
                                source_index,
                                descriptors[source_pos].clone(),
                            ),
                        };
                    precedence_reasons.push(PrecedenceReason {
                        source: descriptors[source_pos].clone(),
                        direction: declaration.direction(),
                        target_set: SystemSetDiagnosticDescriptor::new(declaration.target().name()),
                        target_set_key: declaration.target(),
                        presence: declaration.presence(),
                        predecessor_system_index,
                        predecessor,
                        successor_system_index,
                        successor,
                    });
                }
            }
        }

        if !unresolved.is_empty() {
            unresolved.sort_by(compare_unresolved);
            let first = unresolved
                .into_iter()
                .next()
                .expect("non-empty unresolved list has a first entry");
            return Err(ScheduleValidationError::UnresolvedOrderingReference {
                schedule: label.name(),
                source_system: first.source,
                direction: first.declaration.direction(),
                target_set: SystemSetDiagnosticDescriptor::new(first.declaration.target().name()),
            });
        }

        let mut scheduled_position_by_system_index = vec![None; self.systems.len()];
        for (position, system_index) in scheduled_indices.iter().copied().enumerate() {
            scheduled_position_by_system_index[system_index] = Some(position);
        }

        let mut outgoing = vec![BTreeSet::<usize>::new(); scheduled_indices.len()];
        let mut incoming = vec![0usize; scheduled_indices.len()];
        for reason in &precedence_reasons {
            let predecessor_pos = scheduled_position_by_system_index
                [reason.predecessor_system_index]
                .expect("precedence predecessor belongs to built schedule");
            let successor_pos = scheduled_position_by_system_index[reason.successor_system_index]
                .expect("precedence successor belongs to built schedule");
            if outgoing[predecessor_pos].insert(successor_pos) {
                incoming[successor_pos] = incoming[successor_pos].saturating_add(1);
            }
        }

        let mut ready = BTreeSet::new();
        for (position, indegree) in incoming.iter().enumerate() {
            if *indegree == 0 {
                ready.insert(position);
            }
        }

        let mut stages = Vec::new();
        let mut scheduled_count = 0usize;
        while !ready.is_empty() {
            let stage_positions = ready.iter().copied().collect::<Vec<_>>();
            ready.clear();

            let mut system_indices = Vec::with_capacity(stage_positions.len());
            for position in &stage_positions {
                let system_index = scheduled_indices[*position];
                system_indices.push(system_index);
            }
            scheduled_count = scheduled_count.saturating_add(stage_positions.len());

            for position in stage_positions {
                for dependent in outgoing[position].iter().copied() {
                    incoming[dependent] = incoming[dependent].saturating_sub(1);
                    if incoming[dependent] == 0 {
                        ready.insert(dependent);
                    }
                }
            }

            stages.push(ExecutionStage { system_indices });
        }

        if scheduled_count != scheduled_indices.len() {
            return Err(ScheduleValidationError::OrderingCycle {
                schedule: label.name(),
            });
        }

        Ok(ExecutionPlan {
            label,
            stages,
            ordering_resolutions,
            precedence_reasons,
        })
    }

    fn system_descriptors(&self, scheduled_indices: &[usize]) -> Vec<SystemDiagnosticDescriptor> {
        scheduled_indices
            .iter()
            .copied()
            .enumerate()
            .map(|(position, system_index)| {
                let name = self.systems[system_index].name();
                let same_name_occurrence = scheduled_indices[..position]
                    .iter()
                    .filter(|prior_index| self.systems[**prior_index].name() == name)
                    .count()
                    .saturating_add(1);
                SystemDiagnosticDescriptor::new(name.to_owned(), same_name_occurrence)
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct UnresolvedOrderingReference {
    source: SystemDiagnosticDescriptor,
    declaration: OrderingDeclaration,
}

fn compare_declarations(left: &OrderingDeclaration, right: &OrderingDeclaration) -> CmpOrdering {
    left.direction()
        .cmp(&right.direction())
        .then_with(|| left.target().name().cmp(right.target().name()))
        .then_with(|| left.target().type_id().cmp(&right.target().type_id()))
}

fn compare_unresolved(
    left: &UnresolvedOrderingReference,
    right: &UnresolvedOrderingReference,
) -> CmpOrdering {
    left.source
        .cmp(&right.source)
        .then_with(|| compare_declarations(&left.declaration, &right.declaration))
}

#[cfg(test)]
mod tests {
    use super::{OrderingResolutionKind, ScheduleRegistry};
    use crate::scheduler::access::SystemAccess;
    use crate::scheduler::label::{ScheduleLabel, SystemSet};
    use crate::scheduler::system::{OrderingDeclaration, OrderingPresence, RegisteredSystem};
    use crate::system::OrderingDirection;

    #[derive(Copy, Clone)]
    struct Update;
    impl ScheduleLabel for Update {}

    #[derive(Copy, Clone)]
    struct TargetA;
    impl SystemSet for TargetA {}

    #[derive(Copy, Clone)]
    struct TargetB;
    impl SystemSet for TargetB {}

    fn system(name: &'static str) -> RegisteredSystem {
        RegisteredSystem::new::<Update>(name, SystemAccess::new(), |_world| Ok(()))
            .expect("test system should be valid")
    }

    #[test]
    fn absent_optional_resolution_is_retained() {
        let mut registry = ScheduleRegistry::new();
        let mut source = system("source");
        source.add_ordering_declaration(OrderingDeclaration::optional(
            OrderingDirection::Before,
            TargetA::key(),
        ));
        registry.add_system(source).unwrap();

        let plan = registry.plan_for::<Update>().unwrap().unwrap();
        assert_eq!(plan.ordering_resolutions.len(), 1);
        assert!(matches!(
            plan.ordering_resolutions[0].kind,
            OrderingResolutionKind::AbsentOptional
        ));
        assert_eq!(
            plan.ordering_resolutions[0].declaration.presence(),
            OrderingPresence::Optional
        );
        assert!(plan.precedence_reasons.is_empty());
    }

    #[test]
    fn deduplicated_edge_retains_every_semantic_reason() {
        let mut registry = ScheduleRegistry::new();
        let mut source = system("source");
        source.add_ordering_declaration(OrderingDeclaration::optional(
            OrderingDirection::Before,
            TargetA::key(),
        ));
        source.before_set_key(TargetB::key());
        let mut target = system("target");
        target.with_set_key(TargetA::key());
        target.with_set_key(TargetB::key());
        registry.add_system(source).unwrap();
        registry.add_system(target).unwrap();

        let plan = registry.plan_for::<Update>().unwrap().unwrap();
        assert_eq!(plan.ordering_resolutions.len(), 2);
        assert_eq!(plan.precedence_reasons.len(), 2);
        assert_eq!(plan.stages.len(), 2);
        assert_eq!(plan.stages[0].system_indices, vec![0]);
        assert_eq!(plan.stages[1].system_indices, vec![1]);
        assert!(plan.precedence_reasons.iter().all(|reason| {
            reason.predecessor_system_index == 0 && reason.successor_system_index == 1
        }));
        assert!(
            plan.precedence_reasons
                .iter()
                .any(|reason| reason.target_set_key == TargetA::key())
        );
        assert!(
            plan.precedence_reasons
                .iter()
                .any(|reason| reason.target_set_key == TargetB::key())
        );
        assert!(
            plan.precedence_reasons
                .iter()
                .any(|reason| reason.presence == OrderingPresence::Optional)
        );
        assert!(
            plan.precedence_reasons
                .iter()
                .any(|reason| reason.presence == OrderingPresence::Required)
        );
    }
}
