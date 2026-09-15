use crate::World;
use crate::commands::TransferableCommandBuffer;
use crate::errors::RuntimeError;
use crate::scheduler::system::{
    TransferableSystemRunner, WorkerInvocationOutcome, WorkerPanicPhase,
};
use crate::world::{
    FrameworkInvariantKind, ParallelWorldLease, framework_invariant_kind,
    panic_parallel_executor_violation,
};
use std::panic::resume_unwind;

/// Executes one already-planned transferable worker cohort.
///
/// `members` must be supplied in the schedule's snapshot-local reference-rank
/// space. This function owns only physical execution and canonical worker
/// result reconciliation; semantic cohort construction remains the runtime's
/// responsibility.
pub(crate) fn run_worker_cohort(
    world: &mut World,
    mut members: Vec<(usize, &mut TransferableSystemRunner)>,
) -> Result<Vec<(usize, Option<TransferableCommandBuffer>)>, RuntimeError> {
    members.sort_by_key(|(rank, _)| *rank);
    if !members.windows(2).all(|window| window[0].0 < window[1].0) {
        panic_parallel_executor_violation("worker cohort reference ranks must be unique");
    }

    let mut lease = ParallelWorldLease::new(world);
    let mut prepared = Vec::with_capacity(members.len());
    for (rank, runner) in &members {
        prepared.push((*rank, runner.prepare_worker(&lease)?));
    }
    let capacity = lease.capacity();

    let joined = std::thread::scope(|scope| {
        let handles = members
            .into_iter()
            .zip(prepared)
            .map(|((rank, runner), (prepared_rank, mut prepared))| {
                if rank != prepared_rank {
                    panic_parallel_executor_violation(
                        "prepared worker projection lost its reference rank",
                    );
                }
                let capacity = capacity.clone();
                (
                    rank,
                    scope.spawn(move || runner.run_worker(&mut prepared, capacity)),
                )
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .map(|(rank, handle)| (rank, handle.join()))
            .collect::<Vec<_>>()
    });

    let mut reports = Vec::with_capacity(joined.len());
    let mut unexpected_framework_panic = false;
    for (rank, result) in joined {
        match result {
            Ok(report) => reports.push((rank, report)),
            Err(_payload) => unexpected_framework_panic = true,
        }
    }

    // A panic escaping the registered worker runner means its journal report
    // may have been lost. Join/drain is complete, but semantic reconciliation
    // is no longer proven safe.
    if unexpected_framework_panic {
        drop(lease);
        panic_parallel_executor_violation(
            "worker runner escaped its structured invocation outcome",
        );
    }

    let mut journals = Vec::with_capacity(reports.len());
    let mut outcomes = Vec::with_capacity(reports.len());
    for (rank, report) in reports {
        journals.push(report.journal);
        outcomes.push((rank, report.outcome));
    }

    let framework_indices = outcomes
        .iter()
        .enumerate()
        .filter_map(|(index, (_rank, outcome))| is_framework_failure(outcome).then_some(index))
        .collect::<Vec<_>>();

    if let Some(&selected_index) = framework_indices.first() {
        let every_framework_failure_is_cursor_exhaustion = framework_indices
            .iter()
            .all(|index| is_cursor_exhaustion(&outcomes[*index].1));

        if every_framework_failure_is_cursor_exhaustion {
            // Cursor exhaustion is the accepted invariant whose already-admitted
            // journal prefix remains safe and required to reconcile.
            for journal in journals {
                lease.reconcile(journal);
            }
        }
        drop(lease);

        let selected = outcomes.into_iter().nth(selected_index).unwrap_or_else(|| {
            panic_parallel_executor_violation("selected framework failure disappeared")
        });
        let WorkerInvocationOutcome::Panic { payload, .. } = selected.1 else {
            panic_parallel_executor_violation("framework failure was not represented as a panic")
        };
        resume_unwind(payload);
    }

    // Ordinary user/system failure still requires truthful canonical change
    // bookkeeping for every admitted event from every launched worker.
    for journal in journals {
        lease.reconcile(journal);
    }
    drop(lease);

    let mut buffers = Vec::with_capacity(outcomes.len());
    for (rank, outcome) in outcomes {
        match outcome {
            WorkerInvocationOutcome::Success(buffer) => buffers.push((rank, buffer)),
            WorkerInvocationOutcome::Error(error) => return Err(error),
            WorkerInvocationOutcome::Panic {
                payload,
                phase: WorkerPanicPhase::User,
            } => resume_unwind(payload),
            WorkerInvocationOutcome::Panic {
                phase: WorkerPanicPhase::Framework,
                ..
            } => panic_parallel_executor_violation("framework panic escaped invariant selection"),
        }
    }
    Ok(buffers)
}

fn is_framework_failure(outcome: &WorkerInvocationOutcome) -> bool {
    match outcome {
        WorkerInvocationOutcome::Panic { payload, phase } => {
            *phase == WorkerPanicPhase::Framework
                || framework_invariant_kind(payload.as_ref()).is_some()
        }
        WorkerInvocationOutcome::Success(_) | WorkerInvocationOutcome::Error(_) => false,
    }
}

fn is_cursor_exhaustion(outcome: &WorkerInvocationOutcome) -> bool {
    match outcome {
        WorkerInvocationOutcome::Panic { payload, .. } => {
            framework_invariant_kind(payload.as_ref())
                == Some(FrameworkInvariantKind::ChangeCursorExhausted)
        }
        WorkerInvocationOutcome::Success(_) | WorkerInvocationOutcome::Error(_) => false,
    }
}
