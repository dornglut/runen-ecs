use crate::scheduler::access::{AccessDomain, ConflictKind, SystemAccess};
use crate::scheduler::plan::{
    ExecutionPlan, OrderingResolutionKind, PrecedenceReason, PublicationObligationReason,
};
use crate::scheduler::system::{OrderingPresence, RegisteredSystem};
use crate::system::{
    ExecutionMobility, OrderingDirection, ScheduleDiagnosticDescriptor, SystemDiagnosticDescriptor,
    SystemSetDiagnosticDescriptor,
};
use std::collections::VecDeque;
use std::fmt;

#[derive(Clone)]
pub struct ScheduleInspection {
    schedule: ScheduleDiagnosticDescriptor,
    systems: Vec<SystemDiagnosticDescriptor>,
    ordering_resolutions: Vec<ScheduleOrderingResolution>,
    precedence_edges: Vec<SchedulePrecedenceEdge>,
    publication_frontiers: Vec<SchedulePublicationFrontier>,
    access_ambiguities: Vec<ScheduleAccessAmbiguity>,
    scheduled_descriptors: Vec<SystemDiagnosticDescriptor>,
    accesses: Vec<SystemAccess>,
    execution_mobility: Vec<ExecutionMobility>,
}

impl fmt::Debug for ScheduleInspection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScheduleInspection")
            .field("schedule", &self.schedule)
            .field("systems", &self.systems)
            .field("ordering_resolutions", &self.ordering_resolutions)
            .field("precedence_edges", &self.precedence_edges)
            .field("publication_frontiers", &self.publication_frontiers)
            .field("access_ambiguities", &self.access_ambiguities)
            .field("execution_mobility", &self.execution_mobility)
            .finish()
    }
}

impl ScheduleInspection {
    pub(crate) fn from_plan(plan: &ExecutionPlan, systems: &[RegisteredSystem]) -> Self {
        let scheduled_descriptors = plan.system_descriptors.clone();
        let presentation_systems = {
            let mut systems = scheduled_descriptors.clone();
            systems.sort();
            systems
        };
        let ordering_resolutions = plan
            .ordering_resolutions
            .iter()
            .map(|resolution| {
                let kind = match &resolution.kind {
                    OrderingResolutionKind::Resolved { target_systems, .. } => {
                        let mut target_systems = target_systems.clone();
                        target_systems.sort();
                        ScheduleOrderingResolutionKind::Resolved { target_systems }
                    }
                    OrderingResolutionKind::AbsentOptional => {
                        ScheduleOrderingResolutionKind::AbsentOptional
                    }
                };
                ScheduleOrderingResolution {
                    source: resolution.source.clone(),
                    direction: resolution.declaration.direction(),
                    target_set: resolution.target_set,
                    presence: resolution.declaration.presence(),
                    kind,
                }
            })
            .collect::<Vec<_>>();

        let mut edge_reasons = Vec::<(
            SystemDiagnosticDescriptor,
            SystemDiagnosticDescriptor,
            Vec<_>,
        )>::new();
        for reason in &plan.precedence_reasons {
            let predecessor = reason.predecessor.clone();
            let successor = reason.successor.clone();
            if let Some((_, _, reasons)) =
                edge_reasons
                    .iter_mut()
                    .find(|(candidate_predecessor, candidate_successor, _)| {
                        *candidate_predecessor == predecessor && *candidate_successor == successor
                    })
            {
                reasons.push(public_reason(reason));
            } else {
                edge_reasons.push((predecessor, successor, vec![public_reason(reason)]));
            }
        }
        edge_reasons.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        let precedence_edges = edge_reasons
            .into_iter()
            .map(|(predecessor, successor, mut reasons)| {
                reasons.sort_by(compare_reasons);
                SchedulePrecedenceEdge {
                    predecessor,
                    successor,
                    reasons,
                }
            })
            .collect::<Vec<_>>();

        let publication_frontiers = plan
            .publication_frontiers
            .iter()
            .enumerate()
            .map(|(ordinal, frontier)| SchedulePublicationFrontier {
                ordinal,
                obligations: frontier
                    .obligation_indices
                    .iter()
                    .map(|obligation_index| {
                        let obligation = &plan.publication_obligations[*obligation_index];
                        match &obligation.reason {
                            PublicationObligationReason::Precedence(reason_index) => {
                                SchedulePublicationObligation::Precedence {
                                    reason: public_reason(&plan.precedence_reasons[*reason_index]),
                                }
                            }
                            PublicationObligationReason::Completion { system_index } => {
                                let position = plan
                                    .scheduled_system_indices
                                    .iter()
                                    .position(|candidate| candidate == system_index)
                                    .expect("completion producer belongs to built schedule");
                                SchedulePublicationObligation::Completion {
                                    producer: plan.system_descriptors[position].clone(),
                                }
                            }
                        }
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();

        let mut inspection = Self {
            schedule: plan.schedule_descriptor,
            systems: presentation_systems,
            ordering_resolutions,
            precedence_edges,
            publication_frontiers,
            access_ambiguities: Vec::new(),
            scheduled_descriptors,
            accesses: plan
                .scheduled_system_indices
                .iter()
                .map(|index| systems[*index].access().clone())
                .collect(),
            execution_mobility: plan
                .scheduled_system_indices
                .iter()
                .map(|index| systems[*index].execution_mobility())
                .collect(),
        };
        inspection.access_ambiguities = inspection
            .pair_indices()
            .filter_map(|(left, right)| {
                let conflicts = inspection.access_conflicts(left, right);
                if conflicts.is_empty()
                    || inspection
                        .precedence_path_by_position(left, right)
                        .is_some()
                    || inspection
                        .precedence_path_by_position(right, left)
                        .is_some()
                {
                    return None;
                }
                let (first, second) = if inspection.scheduled_descriptors[left]
                    <= inspection.scheduled_descriptors[right]
                {
                    (left, right)
                } else {
                    (right, left)
                };
                Some(ScheduleAccessAmbiguity {
                    first: inspection.scheduled_descriptors[first].clone(),
                    second: inspection.scheduled_descriptors[second].clone(),
                    conflicts,
                })
            })
            .collect();
        inspection.access_ambiguities.sort_by(|left, right| {
            left.first
                .cmp(&right.first)
                .then_with(|| left.second.cmp(&right.second))
        });
        inspection
    }

    pub fn schedule(&self) -> ScheduleDiagnosticDescriptor {
        self.schedule
    }

    /// Diagnostic presentation order only; this is not execution order.
    pub fn systems(&self) -> &[SystemDiagnosticDescriptor] {
        &self.systems
    }

    pub fn ordering_resolutions(&self) -> &[ScheduleOrderingResolution] {
        &self.ordering_resolutions
    }

    pub fn precedence_edges(&self) -> &[SchedulePrecedenceEdge] {
        &self.precedence_edges
    }

    pub fn publication_frontiers(&self) -> &[SchedulePublicationFrontier] {
        &self.publication_frontiers
    }

    pub fn access_ambiguities(&self) -> &[ScheduleAccessAmbiguity] {
        &self.access_ambiguities
    }

    /// Returns the registered execution capability for a system in this snapshot.
    pub fn execution_mobility(
        &self,
        system: &SystemDiagnosticDescriptor,
    ) -> Option<ExecutionMobility> {
        self.position(system)
            .and_then(|position| self.execution_mobility.get(position).copied())
    }

    pub fn precedence_path(
        &self,
        predecessor: &SystemDiagnosticDescriptor,
        successor: &SystemDiagnosticDescriptor,
    ) -> Option<SchedulePrecedencePath> {
        let predecessor_position = self.position(predecessor)?;
        let successor_position = self.position(successor)?;
        self.precedence_path_by_position(predecessor_position, successor_position)
    }

    pub fn pairwise_concurrency(
        &self,
        first: &SystemDiagnosticDescriptor,
        second: &SystemDiagnosticDescriptor,
    ) -> Option<SchedulePairwiseConcurrencyAssessment> {
        let first_position = self.position(first)?;
        let second_position = self.position(second)?;
        if first_position == second_position {
            return None;
        }
        let precedence = self
            .precedence_path_by_position(first_position, second_position)
            .or_else(|| self.precedence_path_by_position(second_position, first_position));
        Some(SchedulePairwiseConcurrencyAssessment {
            first: first.clone(),
            second: second.clone(),
            precedence,
            access_conflicts: self.access_conflicts(first_position, second_position),
        })
    }

    fn position(&self, descriptor: &SystemDiagnosticDescriptor) -> Option<usize> {
        self.scheduled_descriptors
            .iter()
            .position(|candidate| candidate == descriptor)
    }

    fn pair_indices(&self) -> impl Iterator<Item = (usize, usize)> {
        (0..self.scheduled_descriptors.len()).flat_map(move |left| {
            (left + 1..self.scheduled_descriptors.len()).map(move |right| (left, right))
        })
    }

    fn access_conflicts(&self, left: usize, right: usize) -> Vec<ScheduleAccessConflict> {
        let mut conflicts = self.accesses[left]
            .conflicts_with(&self.accesses[right])
            .into_iter()
            .map(|conflict| ScheduleAccessConflict {
                domain: match conflict.key.domain() {
                    AccessDomain::Component => ScheduleAccessDomain::Component,
                    AccessDomain::RemovedComponent => ScheduleAccessDomain::RemovedComponent,
                    AccessDomain::Resource => ScheduleAccessDomain::Resource,
                    AccessDomain::Relation => ScheduleAccessDomain::Relation,
                    AccessDomain::Structural => ScheduleAccessDomain::Structural,
                    AccessDomain::World => ScheduleAccessDomain::World,
                },
                target: conflict.key.name().to_owned(),
                kind: match conflict.kind {
                    ConflictKind::ReadWrite => ScheduleAccessConflictKind::ReadWrite,
                    ConflictKind::WriteWrite => ScheduleAccessConflictKind::WriteWrite,
                },
            })
            .collect::<Vec<_>>();
        conflicts.sort_by(|left, right| {
            left.domain
                .cmp(&right.domain)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.target.cmp(&right.target))
        });
        conflicts.dedup();
        conflicts
    }

    fn precedence_path_by_position(
        &self,
        predecessor: usize,
        successor: usize,
    ) -> Option<SchedulePrecedencePath> {
        if predecessor == successor {
            return None;
        }
        let edge_indices = self.edge_indices_by_predecessor();
        let mut queue = VecDeque::new();
        let mut visited = vec![false; self.scheduled_descriptors.len()];
        let mut paths = vec![None::<Vec<usize>>; self.scheduled_descriptors.len()];
        visited[predecessor] = true;
        paths[predecessor] = Some(vec![predecessor]);
        queue.push_back(predecessor);
        while let Some(position) = queue.pop_front() {
            let mut outgoing = edge_indices[position].clone();
            outgoing.sort_by(|left, right| {
                self.precedence_edges[*left]
                    .successor
                    .cmp(&self.precedence_edges[*right].successor)
            });
            for edge_index in outgoing {
                let next = self
                    .position(&self.precedence_edges[edge_index].successor)
                    .expect("edge endpoint belongs to inspection snapshot");
                if visited[next] {
                    continue;
                }
                let mut path = paths[position]
                    .as_ref()
                    .expect("visited position has a path")
                    .clone();
                path.push(next);
                if next == successor {
                    let edges = path
                        .windows(2)
                        .map(|pair| {
                            self.precedence_edges
                                .iter()
                                .find(|edge| {
                                    self.position(&edge.predecessor) == Some(pair[0])
                                        && self.position(&edge.successor) == Some(pair[1])
                                })
                                .expect("path edge belongs to inspection snapshot")
                                .clone()
                        })
                        .collect();
                    return Some(SchedulePrecedencePath {
                        systems: path
                            .into_iter()
                            .map(|position| self.scheduled_descriptors[position].clone())
                            .collect(),
                        edges,
                    });
                }
                visited[next] = true;
                paths[next] = Some(path);
                queue.push_back(next);
            }
        }
        None
    }

    fn edge_indices_by_predecessor(&self) -> Vec<Vec<usize>> {
        let mut indices = vec![Vec::new(); self.scheduled_descriptors.len()];
        for (edge_index, edge) in self.precedence_edges.iter().enumerate() {
            if let Some(position) = self.position(&edge.predecessor) {
                indices[position].push(edge_index);
            }
        }
        indices
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleOrderingResolution {
    source: SystemDiagnosticDescriptor,
    direction: OrderingDirection,
    target_set: SystemSetDiagnosticDescriptor,
    presence: OrderingPresence,
    kind: ScheduleOrderingResolutionKind,
}

impl ScheduleOrderingResolution {
    pub fn source(&self) -> &SystemDiagnosticDescriptor {
        &self.source
    }
    pub const fn direction(&self) -> OrderingDirection {
        self.direction
    }
    pub const fn target_set(&self) -> SystemSetDiagnosticDescriptor {
        self.target_set
    }
    pub const fn presence(&self) -> OrderingPresence {
        self.presence
    }
    pub fn kind(&self) -> &ScheduleOrderingResolutionKind {
        &self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleOrderingResolutionKind {
    Resolved {
        target_systems: Vec<SystemDiagnosticDescriptor>,
    },
    AbsentOptional,
}

impl ScheduleOrderingResolutionKind {
    pub fn target_systems(&self) -> Option<&[SystemDiagnosticDescriptor]> {
        match self {
            Self::Resolved { target_systems } => Some(target_systems),
            Self::AbsentOptional => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulePrecedenceReason {
    source: SystemDiagnosticDescriptor,
    direction: OrderingDirection,
    target_set: SystemSetDiagnosticDescriptor,
    presence: OrderingPresence,
    predecessor: SystemDiagnosticDescriptor,
    successor: SystemDiagnosticDescriptor,
}

impl SchedulePrecedenceReason {
    pub fn source(&self) -> &SystemDiagnosticDescriptor {
        &self.source
    }
    pub const fn direction(&self) -> OrderingDirection {
        self.direction
    }
    pub const fn target_set(&self) -> SystemSetDiagnosticDescriptor {
        self.target_set
    }
    pub const fn presence(&self) -> OrderingPresence {
        self.presence
    }
    pub fn predecessor(&self) -> &SystemDiagnosticDescriptor {
        &self.predecessor
    }
    pub fn successor(&self) -> &SystemDiagnosticDescriptor {
        &self.successor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulePrecedenceEdge {
    predecessor: SystemDiagnosticDescriptor,
    successor: SystemDiagnosticDescriptor,
    reasons: Vec<SchedulePrecedenceReason>,
}

impl SchedulePrecedenceEdge {
    pub fn predecessor(&self) -> &SystemDiagnosticDescriptor {
        &self.predecessor
    }
    pub fn successor(&self) -> &SystemDiagnosticDescriptor {
        &self.successor
    }
    pub fn reasons(&self) -> &[SchedulePrecedenceReason] {
        &self.reasons
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulePrecedencePath {
    systems: Vec<SystemDiagnosticDescriptor>,
    edges: Vec<SchedulePrecedenceEdge>,
}

impl SchedulePrecedencePath {
    pub fn systems(&self) -> &[SystemDiagnosticDescriptor] {
        &self.systems
    }
    pub fn edges(&self) -> &[SchedulePrecedenceEdge] {
        &self.edges
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulePublicationFrontier {
    ordinal: usize,
    obligations: Vec<SchedulePublicationObligation>,
}

impl SchedulePublicationFrontier {
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }
    pub fn obligations(&self) -> &[SchedulePublicationObligation] {
        &self.obligations
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulePublicationObligation {
    Precedence {
        reason: SchedulePrecedenceReason,
    },
    Completion {
        producer: SystemDiagnosticDescriptor,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScheduleAccessDomain {
    Component,
    RemovedComponent,
    Resource,
    Relation,
    Structural,
    World,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScheduleAccessConflictKind {
    ReadWrite,
    WriteWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScheduleAccessConflict {
    domain: ScheduleAccessDomain,
    target: String,
    kind: ScheduleAccessConflictKind,
}

impl ScheduleAccessConflict {
    pub const fn domain(&self) -> ScheduleAccessDomain {
        self.domain
    }
    pub fn target(&self) -> &str {
        &self.target
    }
    pub const fn kind(&self) -> ScheduleAccessConflictKind {
        self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleAccessAmbiguity {
    first: SystemDiagnosticDescriptor,
    second: SystemDiagnosticDescriptor,
    conflicts: Vec<ScheduleAccessConflict>,
}

impl ScheduleAccessAmbiguity {
    pub fn first(&self) -> &SystemDiagnosticDescriptor {
        &self.first
    }
    pub fn second(&self) -> &SystemDiagnosticDescriptor {
        &self.second
    }
    pub fn conflicts(&self) -> &[ScheduleAccessConflict] {
        &self.conflicts
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulePairwiseConcurrencyAssessment {
    first: SystemDiagnosticDescriptor,
    second: SystemDiagnosticDescriptor,
    precedence: Option<SchedulePrecedencePath>,
    access_conflicts: Vec<ScheduleAccessConflict>,
}

impl SchedulePairwiseConcurrencyAssessment {
    pub fn first(&self) -> &SystemDiagnosticDescriptor {
        &self.first
    }
    pub fn second(&self) -> &SystemDiagnosticDescriptor {
        &self.second
    }
    pub fn precedence_path(&self) -> Option<&SchedulePrecedencePath> {
        self.precedence.as_ref()
    }
    pub fn access_conflicts(&self) -> &[ScheduleAccessConflict] {
        &self.access_conflicts
    }
    pub fn is_prevented(&self) -> bool {
        self.precedence.is_some() || !self.access_conflicts.is_empty()
    }
    pub fn is_unconstrained(&self) -> bool {
        !self.is_prevented()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleOrderingCycle {
    reasons: Vec<SchedulePrecedenceReason>,
}

impl ScheduleOrderingCycle {
    pub(crate) fn new(reasons: Vec<SchedulePrecedenceReason>) -> Self {
        Self { reasons }
    }
    pub fn reasons(&self) -> &[SchedulePrecedenceReason] {
        &self.reasons
    }
}

pub(crate) fn canonical_cycle(
    descriptors: &[SystemDiagnosticDescriptor],
    reasons: &[PrecedenceReason],
) -> ScheduleOrderingCycle {
    let mut outgoing = vec![Vec::<usize>::new(); descriptors.len()];
    for reason in reasons {
        let predecessor = descriptors
            .iter()
            .position(|descriptor| *descriptor == reason.predecessor)
            .expect("cycle predecessor belongs to schedule");
        let successor = descriptors
            .iter()
            .position(|descriptor| *descriptor == reason.successor)
            .expect("cycle successor belongs to schedule");
        if !outgoing[predecessor].contains(&successor) {
            outgoing[predecessor].push(successor);
        }
    }
    for successors in &mut outgoing {
        successors.sort_by(|left, right| descriptors[*left].cmp(&descriptors[*right]));
    }
    let mut ordered = (0..descriptors.len()).collect::<Vec<_>>();
    ordered.sort_by(|left, right| descriptors[*left].cmp(&descriptors[*right]));
    for start in ordered {
        let mut visited = vec![false; descriptors.len()];
        let mut path = vec![start];
        visited[start] = true;
        if let Some(cycle) = find_cycle(
            start,
            start,
            &outgoing,
            descriptors,
            &mut visited,
            &mut path,
        ) {
            let cycle_reasons = cycle
                .windows(2)
                .chain(std::iter::once(&[cycle[cycle.len() - 1], cycle[0]][..]))
                .map(|edge| {
                    reasons
                        .iter()
                        .filter(|reason| {
                            reason.predecessor == descriptors[edge[0]]
                                && reason.successor == descriptors[edge[1]]
                        })
                        .min_by(compare_internal_reasons)
                        .expect("cycle edge has a semantic reason")
                        .clone()
                })
                .map(|reason| public_reason(&reason))
                .collect();
            return ScheduleOrderingCycle::new(cycle_reasons);
        }
    }
    ScheduleOrderingCycle::new(Vec::new())
}

fn find_cycle(
    start: usize,
    current: usize,
    outgoing: &[Vec<usize>],
    descriptors: &[SystemDiagnosticDescriptor],
    visited: &mut [bool],
    path: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    for successor in &outgoing[current] {
        if *successor == start && path.len() > 1 {
            return Some(path.clone());
        }
        if !visited[*successor] && descriptors[*successor] >= descriptors[start] {
            visited[*successor] = true;
            path.push(*successor);
            if let Some(cycle) = find_cycle(start, *successor, outgoing, descriptors, visited, path)
            {
                return Some(cycle);
            }
            path.pop();
            visited[*successor] = false;
        }
    }
    None
}

fn public_reason(reason: &PrecedenceReason) -> SchedulePrecedenceReason {
    SchedulePrecedenceReason {
        source: reason.source.clone(),
        direction: reason.direction,
        target_set: reason.target_set,
        presence: reason.presence,
        predecessor: reason.predecessor.clone(),
        successor: reason.successor.clone(),
    }
}

fn compare_reasons(
    left: &SchedulePrecedenceReason,
    right: &SchedulePrecedenceReason,
) -> std::cmp::Ordering {
    left.source
        .cmp(&right.source)
        .then_with(|| left.direction.cmp(&right.direction))
        .then_with(|| left.target_set.cmp(&right.target_set))
        .then_with(|| left.presence.cmp(&right.presence))
        .then_with(|| left.predecessor.cmp(&right.predecessor))
        .then_with(|| left.successor.cmp(&right.successor))
}

fn compare_internal_reasons(
    left: &&PrecedenceReason,
    right: &&PrecedenceReason,
) -> std::cmp::Ordering {
    left.source
        .cmp(&right.source)
        .then_with(|| left.direction.cmp(&right.direction))
        .then_with(|| left.target_set.cmp(&right.target_set))
        .then_with(|| left.presence.cmp(&right.presence))
        .then_with(|| left.predecessor.cmp(&right.predecessor))
        .then_with(|| left.successor.cmp(&right.successor))
}
