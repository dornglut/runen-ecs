use crate::scheduler::label::{ScheduleKey, ScheduleLabel, SystemSetKey};
use crate::scheduler::system::{OrderingDeclaration, OrderingPresence, RegisteredSystem, SystemId};
use crate::system::{
    OrderingDirection, ScheduleDiagnosticDescriptor, SystemDiagnosticDescriptor,
    SystemSetDiagnosticDescriptor,
};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BTreeSet;
use thiserror::Error;

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
    pub(crate) target_set: SystemSetDiagnosticDescriptor,
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
    pub(crate) reference_system_indices: Vec<usize>,
    #[allow(dead_code)]
    pub(crate) reference_rank_by_system_index: Vec<Option<usize>>,
    #[allow(dead_code)]
    pub(crate) ordering_resolutions: Vec<OrderingResolution>,
    #[allow(dead_code)]
    pub(crate) precedence_reasons: Vec<PrecedenceReason>,
    #[allow(dead_code)]
    pub(crate) publication_obligations: Vec<PublicationObligation>,
    pub(crate) publication_frontiers: Vec<PublicationFrontierPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublicationObligationReason {
    Precedence(usize),
    Completion { system_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicationObligation {
    pub(crate) producer_rank: usize,
    pub(crate) deadline_cut: usize,
    pub(crate) reason: PublicationObligationReason,
    pub(crate) frontier_ordinal: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicationFrontierPlan {
    pub(crate) cut: usize,
    pub(crate) obligation_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleValidationError {
    #[error(
        "schedule '{schedule}' system '{source_system}' has unresolved required {direction} reference to set '{target_set}'"
    )]
    UnresolvedOrderingReference {
        schedule: ScheduleDiagnosticDescriptor,
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
        let set_descriptors = self.system_set_descriptors(&scheduled_indices);

        let mut ordering_resolutions = Vec::new();
        let mut precedence_reasons = Vec::new();
        let mut unresolved = Vec::new();

        for (source_pos, source_index) in scheduled_indices.iter().copied().enumerate() {
            let source = &self.systems[source_index];
            let mut declarations = source.ordering_declarations().to_vec();
            declarations.sort_by(|left, right| compare_declarations(left, right, &set_descriptors));

            for declaration in declarations {
                let target_set = system_set_descriptor(&set_descriptors, declaration.target());
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
                            target_set,
                        });
                    } else {
                        ordering_resolutions.push(OrderingResolution {
                            source_system_index: source_index,
                            source: descriptors[source_pos].clone(),
                            declaration,
                            target_set,
                            kind: OrderingResolutionKind::AbsentOptional,
                        });
                    }
                    continue;
                }

                ordering_resolutions.push(OrderingResolution {
                    source_system_index: source_index,
                    source: descriptors[source_pos].clone(),
                    declaration,
                    target_set,
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
                        target_set,
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
                schedule: self.schedule_descriptor(label),
                source_system: first.source,
                direction: first.declaration.direction(),
                target_set: first.target_set,
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

        let mut precedence_depth = vec![0usize; scheduled_indices.len()];
        let mut scheduled_count = 0usize;
        while !ready.is_empty() {
            let position = *ready
                .first()
                .expect("non-empty ready set has a first position");
            ready.remove(&position);
            scheduled_count = scheduled_count.saturating_add(1);

            for dependent in outgoing[position].iter().copied() {
                precedence_depth[dependent] =
                    precedence_depth[dependent].max(precedence_depth[position].saturating_add(1));
                incoming[dependent] = incoming[dependent].saturating_sub(1);
                if incoming[dependent] == 0 {
                    ready.insert(dependent);
                }
            }
        }

        if scheduled_count != scheduled_indices.len() {
            return Err(ScheduleValidationError::OrderingCycle {
                schedule: label.name(),
            });
        }

        let mut reference_positions = (0..scheduled_indices.len()).collect::<Vec<_>>();
        reference_positions.sort_by_key(|position| (precedence_depth[*position], *position));
        let reference_system_indices = reference_positions
            .iter()
            .map(|position| scheduled_indices[*position])
            .collect::<Vec<_>>();
        let mut reference_rank_by_system_index = vec![None; self.systems.len()];
        for (rank, system_index) in reference_system_indices.iter().copied().enumerate() {
            reference_rank_by_system_index[system_index] = Some(rank);
        }

        let mut publication_obligations = Vec::new();
        for (reason_index, reason) in precedence_reasons.iter().enumerate() {
            let producer = &self.systems[reason.predecessor_system_index];
            if !producer.deferred_recorder_class().is_deferred_producing() {
                continue;
            }
            let producer_rank = reference_rank_by_system_index[reason.predecessor_system_index]
                .expect("precedence producer belongs to built schedule");
            let deadline_cut = reference_rank_by_system_index[reason.successor_system_index]
                .expect("precedence successor belongs to built schedule");
            publication_obligations.push(PublicationObligation {
                producer_rank,
                deadline_cut,
                reason: PublicationObligationReason::Precedence(reason_index),
                frontier_ordinal: usize::MAX,
            });
        }
        for system_index in reference_system_indices.iter().copied() {
            let system = &self.systems[system_index];
            if !system.deferred_recorder_class().is_deferred_producing() {
                continue;
            }
            let producer_rank = reference_rank_by_system_index[system_index]
                .expect("deferred producer belongs to built schedule");
            publication_obligations.push(PublicationObligation {
                producer_rank,
                deadline_cut: reference_system_indices.len(),
                reason: PublicationObligationReason::Completion { system_index },
                frontier_ordinal: usize::MAX,
            });
        }

        let mut obligation_order = (0..publication_obligations.len()).collect::<Vec<_>>();
        obligation_order.sort_by(|left, right| {
            let left_obligation = &publication_obligations[*left];
            let right_obligation = &publication_obligations[*right];
            left_obligation
                .deadline_cut
                .cmp(&right_obligation.deadline_cut)
                .then_with(|| {
                    compare_publication_reasons(
                        &left_obligation.reason,
                        &right_obligation.reason,
                        &precedence_reasons,
                        &self.systems,
                    )
                })
                .then_with(|| left.cmp(right))
        });

        let mut selected_cuts = Vec::new();
        for obligation_index in obligation_order {
            let obligation = &publication_obligations[obligation_index];
            let covered = selected_cuts
                .iter()
                .any(|cut| *cut > obligation.producer_rank && *cut <= obligation.deadline_cut);
            if !covered && !selected_cuts.contains(&obligation.deadline_cut) {
                selected_cuts.push(obligation.deadline_cut);
            }
        }
        selected_cuts.sort_unstable();

        let publication_frontiers = selected_cuts
            .iter()
            .copied()
            .map(|cut| PublicationFrontierPlan {
                cut,
                obligation_indices: Vec::new(),
            })
            .collect::<Vec<_>>();

        for obligation in &mut publication_obligations {
            let frontier_ordinal = publication_frontiers
                .iter()
                .position(|frontier| {
                    frontier.cut > obligation.producer_rank
                        && frontier.cut <= obligation.deadline_cut
                })
                .expect("every publication obligation has a selected frontier");
            obligation.frontier_ordinal = frontier_ordinal;
        }

        let mut publication_frontiers = publication_frontiers;
        for (obligation_index, obligation) in publication_obligations.iter().enumerate() {
            publication_frontiers[obligation.frontier_ordinal]
                .obligation_indices
                .push(obligation_index);
        }

        Ok(ExecutionPlan {
            label,
            reference_system_indices,
            reference_rank_by_system_index,
            ordering_resolutions,
            precedence_reasons,
            publication_obligations,
            publication_frontiers,
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

    fn schedule_descriptor(&self, label: ScheduleKey) -> ScheduleDiagnosticDescriptor {
        let mut labels = Vec::<ScheduleKey>::new();
        for system in &self.systems {
            if !labels.iter().any(|existing| *existing == system.label()) {
                labels.push(system.label());
            }
        }
        labels.sort_by(compare_schedule_keys_for_diagnostics);
        let position = labels
            .iter()
            .position(|candidate| *candidate == label)
            .expect("built schedule label belongs to registry");
        let occurrence = labels[..position]
            .iter()
            .filter(|prior| same_schedule_diagnostic_text(**prior, label))
            .count()
            .saturating_add(1);
        ScheduleDiagnosticDescriptor::new(label.name(), label.diagnostic_type_name(), occurrence)
    }

    fn system_set_descriptors(
        &self,
        scheduled_indices: &[usize],
    ) -> Vec<(SystemSetKey, SystemSetDiagnosticDescriptor)> {
        let mut keys = Vec::<SystemSetKey>::new();
        for system_index in scheduled_indices.iter().copied() {
            let system = &self.systems[system_index];
            for key in system.sets().iter().copied().chain(
                system
                    .ordering_declarations()
                    .iter()
                    .map(|declaration| declaration.target()),
            ) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        keys.sort_by(compare_system_set_keys_for_diagnostics);
        keys.iter()
            .copied()
            .enumerate()
            .map(|(position, key)| {
                let occurrence = keys[..position]
                    .iter()
                    .filter(|prior| same_system_set_diagnostic_text(**prior, key))
                    .count()
                    .saturating_add(1);
                (
                    key,
                    SystemSetDiagnosticDescriptor::new(
                        key.name(),
                        key.diagnostic_type_name(),
                        occurrence,
                    ),
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct UnresolvedOrderingReference {
    source: SystemDiagnosticDescriptor,
    declaration: OrderingDeclaration,
    target_set: SystemSetDiagnosticDescriptor,
}

fn compare_schedule_keys_for_diagnostics(left: &ScheduleKey, right: &ScheduleKey) -> CmpOrdering {
    left.name()
        .cmp(right.name())
        .then_with(|| {
            left.diagnostic_type_name()
                .cmp(right.diagnostic_type_name())
        })
        .then_with(|| left.type_id().cmp(&right.type_id()))
}

fn compare_system_set_keys_for_diagnostics(
    left: &SystemSetKey,
    right: &SystemSetKey,
) -> CmpOrdering {
    left.name()
        .cmp(right.name())
        .then_with(|| {
            left.diagnostic_type_name()
                .cmp(right.diagnostic_type_name())
        })
        .then_with(|| left.type_id().cmp(&right.type_id()))
}

fn same_schedule_diagnostic_text(left: ScheduleKey, right: ScheduleKey) -> bool {
    left.name() == right.name() && left.diagnostic_type_name() == right.diagnostic_type_name()
}

fn same_system_set_diagnostic_text(left: SystemSetKey, right: SystemSetKey) -> bool {
    left.name() == right.name() && left.diagnostic_type_name() == right.diagnostic_type_name()
}

fn system_set_descriptor(
    descriptors: &[(SystemSetKey, SystemSetDiagnosticDescriptor)],
    key: SystemSetKey,
) -> SystemSetDiagnosticDescriptor {
    descriptors
        .iter()
        .find_map(|(candidate, descriptor)| (*candidate == key).then_some(*descriptor))
        .expect("ordering declaration target belongs to diagnostic catalog")
}

fn compare_declarations(
    left: &OrderingDeclaration,
    right: &OrderingDeclaration,
    descriptors: &[(SystemSetKey, SystemSetDiagnosticDescriptor)],
) -> CmpOrdering {
    left.direction().cmp(&right.direction()).then_with(|| {
        system_set_descriptor(descriptors, left.target())
            .cmp(&system_set_descriptor(descriptors, right.target()))
    })
}

fn compare_unresolved(
    left: &UnresolvedOrderingReference,
    right: &UnresolvedOrderingReference,
) -> CmpOrdering {
    left.source
        .cmp(&right.source)
        .then_with(|| {
            left.declaration
                .direction()
                .cmp(&right.declaration.direction())
        })
        .then_with(|| left.target_set.cmp(&right.target_set))
}

fn compare_publication_reasons(
    left: &PublicationObligationReason,
    right: &PublicationObligationReason,
    precedence_reasons: &[PrecedenceReason],
    systems: &[RegisteredSystem],
) -> CmpOrdering {
    match (left, right) {
        (
            PublicationObligationReason::Precedence(left_index),
            PublicationObligationReason::Precedence(right_index),
        ) => {
            let left_reason = &precedence_reasons[*left_index];
            let right_reason = &precedence_reasons[*right_index];
            left_reason
                .source
                .cmp(&right_reason.source)
                .then_with(|| left_reason.direction.cmp(&right_reason.direction))
                .then_with(|| left_reason.target_set.cmp(&right_reason.target_set))
                .then_with(|| left_reason.presence.cmp(&right_reason.presence))
                .then_with(|| left_reason.predecessor.cmp(&right_reason.predecessor))
                .then_with(|| left_reason.successor.cmp(&right_reason.successor))
                .then_with(|| left_index.cmp(right_index))
        }
        (
            PublicationObligationReason::Completion {
                system_index: left_index,
            },
            PublicationObligationReason::Completion {
                system_index: right_index,
            },
        ) => systems[*left_index]
            .name()
            .cmp(systems[*right_index].name())
            .then_with(|| left_index.cmp(right_index)),
        (
            PublicationObligationReason::Precedence(_),
            PublicationObligationReason::Completion { .. },
        ) => CmpOrdering::Less,
        (
            PublicationObligationReason::Completion { .. },
            PublicationObligationReason::Precedence(_),
        ) => CmpOrdering::Greater,
    }
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
            plan.ordering_resolutions[0].target_set.name(),
            TargetA::key().name()
        );
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
        assert_eq!(plan.reference_system_indices, vec![0, 1]);
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

    #[test]
    fn reference_order_uses_depth_then_schedule_source_ordinal() {
        let mut registry = ScheduleRegistry::new();
        let mut a = system("A");
        a.with_set_key(TargetA::key());
        let c = system("C").after_set::<TargetA>();
        let b = system("B");
        registry.add_system(a).unwrap();
        registry.add_system(c).unwrap();
        registry.add_system(b).unwrap();

        let plan = registry.plan_for::<Update>().unwrap().unwrap();
        assert_eq!(plan.reference_system_indices, vec![0, 2, 1]);
    }

    #[test]
    fn non_deferred_schedule_has_no_publication_frontier() {
        let mut registry = ScheduleRegistry::new();
        registry.add_system(system("plain")).unwrap();

        let plan = registry.plan_for::<Update>().unwrap().unwrap();
        assert!(plan.publication_frontiers.is_empty());
        assert!(plan.publication_obligations.is_empty());
    }

    #[test]
    fn deferred_completion_is_one_frontier_at_schedule_completion() {
        let mut registry = ScheduleRegistry::new();
        let mut producer = system("producer");
        producer.set_deferred_recorder_class(crate::system::DeferredRecorderClass::LocalDeferred);
        registry.add_system(producer).unwrap();

        let plan = registry.plan_for::<Update>().unwrap().unwrap();
        assert_eq!(plan.publication_frontiers.len(), 1);
        assert_eq!(plan.publication_frontiers[0].cut, 1);
        assert_eq!(plan.publication_obligations.len(), 1);
        assert_eq!(plan.publication_obligations[0].frontier_ordinal, 0);
    }

    #[test]
    fn edge_and_completion_obligations_share_an_earlier_frontier() {
        let mut registry = ScheduleRegistry::new();
        let mut producer = system("producer");
        producer.with_set_key(TargetA::key());
        producer.set_deferred_recorder_class(crate::system::DeferredRecorderClass::LocalDeferred);
        let successor = system("successor").after_set::<TargetA>();
        registry.add_system(producer).unwrap();
        registry.add_system(successor).unwrap();

        let plan = registry.plan_for::<Update>().unwrap().unwrap();
        assert_eq!(plan.publication_frontiers.len(), 1);
        assert_eq!(plan.publication_frontiers[0].cut, 1);
        assert_eq!(plan.publication_frontiers[0].obligation_indices, vec![0, 1]);
        assert!(
            plan.publication_obligations
                .iter()
                .all(|obligation| obligation.frontier_ordinal == 0)
        );
    }
}
