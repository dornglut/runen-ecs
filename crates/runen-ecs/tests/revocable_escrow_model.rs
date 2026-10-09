//! Executable bounded model for issue #157's proposed revocable admission escrows.
//!
//! This is deliberately NOT a substitute for tests against an actual threaded
//! admission implementation. Steps represent proposed atomic linearization
//! points and mutex-protected refill/terminal scan transactions. It tests the
//! accounting protocol without introducing another production authority.

use std::collections::{HashSet, VecDeque};

const MAX_WORKERS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Phase {
    Idle,
    Requesting,
    Paused,
    Finished,
    UserPanicked,
    CursorExhausted,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct State {
    unassigned: u8,
    escrow: [u8; MAX_WORKERS],
    admitted: [u8; MAX_WORKERS],
    phase: [Phase; MAX_WORKERS],
    requests: u8,
    foreign_claim: bool,
}

impl State {
    fn initial(capacity: u8, workers: usize) -> Self {
        let mut phase = [Phase::Finished; MAX_WORKERS];
        for slot in &mut phase[..workers] {
            *slot = Phase::Idle;
        }
        Self {
            unassigned: capacity,
            escrow: [0; MAX_WORKERS],
            admitted: [0; MAX_WORKERS],
            phase,
            requests: 0,
            foreign_claim: false,
        }
    }

    fn admitted_total(&self) -> u16 {
        self.admitted.iter().map(|&count| u16::from(count)).sum()
    }

    fn escrow_total(&self) -> u16 {
        self.escrow.iter().map(|&count| u16::from(count)).sum()
    }

    fn assert_invariants(&self, capacity: u8, workers: usize) {
        let admitted = self.admitted_total();
        assert_eq!(
            u16::from(self.unassigned) + self.escrow_total() + admitted,
            u16::from(capacity),
            "an admission, refill or redemption lost or fabricated capacity: {self:?}"
        );
        assert!(admitted <= u16::from(capacity));
        for slot in workers..MAX_WORKERS {
            assert_eq!(self.phase[slot], Phase::Finished);
            assert_eq!(self.escrow[slot], 0);
            assert_eq!(self.admitted[slot], 0);
        }
        if self.phase.contains(&Phase::CursorExhausted) {
            assert_eq!(
                admitted,
                u16::from(capacity),
                "a worker reported false cursor exhaustion: {self:?}"
            );
        }

        // Reference-rank reconciliation replays every admitted local prefix,
        // independent of worker admission order, without skipping positions.
        let mut cursor = 0u16;
        for &events in &self.admitted[..workers] {
            for _ in 0..events {
                cursor += 1;
            }
        }
        assert_eq!(cursor, admitted);
    }

    fn next_for(&self, worker: usize, capacity: u8, quota: u8, workers: usize) -> Vec<Self> {
        match self.phase[worker] {
            Phase::Idle => {
                let mut next = Vec::new();
                if self.requests < capacity + workers as u8 {
                    let mut request = self.clone();
                    request.requests += 1;
                    request.phase[worker] = Phase::Requesting;
                    next.push(request);
                }
                for phase in [Phase::Paused, Phase::Finished, Phase::UserPanicked] {
                    let mut outcome = self.clone();
                    outcome.phase[worker] = phase;
                    next.push(outcome);
                }
                next
            }
            Phase::Paused => {
                [Phase::Idle, Phase::Finished, Phase::UserPanicked]
                    .into_iter()
                    .map(|phase| {
                        let mut next = self.clone();
                        next.phase[worker] = phase;
                        next
                    })
                    .collect()
            }
            Phase::Requesting => {
                // A successful checked CAS is one real event's pre-exposure
                // admission. It cannot be undone or issued twice.
                if self.escrow[worker] != 0 {
                    let mut next = self.clone();
                    next.escrow[worker] -= 1;
                    next.admitted[worker] += 1;
                    next.phase[worker] = Phase::Idle;
                    return vec![next];
                }

                // Under the distribution mutex, these two bookkeeping
                // updates are one transaction to every other refill/scanner.
                // The worker is not running user code while refilling.
                if self.unassigned != 0 {
                    let mut next = self.clone();
                    let grant = next.unassigned.min(quota);
                    next.unassigned -= grant;
                    next.escrow[worker] += grant;
                    return vec![next];
                }

                // With the distribution mutex held and unassigned == 0,
                // no worker may deposit more credits. Donor balances only
                // decrease; steal one credit via CAS or prove exhaustion.
                let mut next = Vec::new();
                for donor in 0..workers {
                    if self.escrow[donor] == 0 {
                        continue;
                    }
                    let mut claimed = self.clone();
                    claimed.escrow[donor] -= 1;
                    claimed.admitted[worker] += 1;
                    claimed.phase[worker] = Phase::Idle;
                    claimed.foreign_claim |= donor != worker;
                    next.push(claimed);
                }
                if next.is_empty() {
                    let mut exhausted = self.clone();
                    exhausted.phase[worker] = Phase::CursorExhausted;
                    next.push(exhausted);
                }
                next
            }
            Phase::Finished | Phase::UserPanicked | Phase::CursorExhausted => Vec::new(),
        }
    }
}

fn explore(capacity: u8, workers: usize, quota: u8) -> HashSet<State> {
    let initial = State::initial(capacity, workers);
    let mut visited = HashSet::from([initial.clone()]);
    let mut pending = VecDeque::from([initial]);

    while let Some(state) = pending.pop_front() {
        state.assert_invariants(capacity, workers);
        for worker in 0..workers {
            for successor in state.next_for(worker, capacity, quota, workers) {
                if visited.insert(successor.clone()) {
                    pending.push_back(successor);
                }
            }
        }
    }
    visited
}

#[test]
fn exhaustive_small_capacity_interleavings_preserve_exact_admission() {
    // All request/refill/claim/foreign-redemption steps, plus arbitrary
    // pauses, resumptions, successful exits, and user panics, are interleaved.
    // At most capacity + workers event requests are necessary to encounter
    // both complete admission and concurrent terminal requests in this model.
    for capacity in 0..=4 {
        for workers in 1..=MAX_WORKERS {
            for quota in 1..=3 {
                let reachable = explore(capacity, workers, quota);
                assert!(
                    !reachable.is_empty(),
                    "missing reachable states for {capacity}/{workers}/{quota}"
                );
                assert!(reachable.iter().any(|state| {
                    state.admitted_total() == u16::from(capacity)
                }));
            }
        }
    }
}

#[test]
fn two_slot_conditional_worker_can_redeem_paused_donor_credit() {
    let mut state = State::initial(2, 2);
    state.phase[0] = Phase::Requesting;
    state.requests = 1;
    state = state.next_for(0, 2, 2, 2).remove(0);
    assert_eq!(state.escrow[0], 2);
    assert_eq!(state.unassigned, 0);

    state = state.next_for(0, 2, 2, 2).remove(0);
    assert_eq!(state.admitted, [1, 0, 0]);

    // A stops making mutation requests, awaiting B in arbitrary user code.
    state.phase[0] = Phase::Paused;
    state.phase[1] = Phase::Requesting;
    state.requests = 2;
    state = state.next_for(1, 2, 2, 2).remove(0);
    assert_eq!(state.admitted, [1, 1, 0]);
    assert_eq!(state.escrow, [0, 0, 0]);
    assert!(state.foreign_claim);
    state.assert_invariants(2, 2);
}

#[test]
fn terminal_claim_is_unique_under_both_interleavings() {
    let mut state = State::initial(1, 2);
    state.phase[0] = Phase::Requesting;
    state.requests = 1;
    state = state.next_for(0, 1, 3, 2).remove(0);
    state.phase[1] = Phase::Requesting;
    state.requests = 2;

    // Owner's CAS wins, then the borrower correctly exhausts.
    let owner_wins = state.next_for(0, 1, 3, 2).remove(0);
    let borrower_exhausts = owner_wins.next_for(1, 1, 3, 2).remove(0);
    assert_eq!(borrower_exhausts.phase[1], Phase::CursorExhausted);
    assert_eq!(borrower_exhausts.admitted, [1, 0, 0]);
    borrower_exhausts.assert_invariants(1, 2);

    // Borrower's CAS wins, then the owner correctly exhausts.
    let borrower_wins = state.next_for(1, 1, 3, 2).remove(0);
    let owner_exhausts = borrower_wins.next_for(0, 1, 3, 2).remove(0);
    assert_eq!(owner_exhausts.phase[0], Phase::CursorExhausted);
    assert_eq!(owner_exhausts.admitted, [0, 1, 0]);
    owner_exhausts.assert_invariants(1, 2);
}

#[test]
fn terminal_u128_cursor_and_u64_tail_use_exact_capacity() {
    for (base, expected) in [
        (u128::MAX, 0),
        (u128::MAX - 1, 1),
        (u128::MAX - 2, 2),
        ((u64::MAX as u128) << 64, u64::MAX as u128),
        (((u64::MAX - 1) as u128) << 64 | u64::MAX as u128, 1u128 << 64),
    ] {
        assert_eq!(u128::MAX - base, expected);
        assert_eq!(base.checked_add(expected), Some(u128::MAX));
        assert_eq!(base.checked_add(expected.saturating_add(1)), None);
    }
}
