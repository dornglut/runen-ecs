# Replay type-publication proof (#152)

This is a **private, semantic-preserving performance experiment**. The
accepted serial oracle, component-change cursor, deterministic journal replay
order, deferred visibility frontiers, secondary-index behavior and exact
exhaustion/panic identity remain authoritative.

## Why test this design

The accepted four-physical-core [ARM phase run](https://github.com/dornglut/runen-ecs/actions/runs/37923498860)
measured ~238–240 ms of strictly ordered journal reconciliation per
one-million-entity, eight-independent-writer schedule (eight million
conservative mutable component exposure events). Invoker replay currently
updates the same per-component high-water cursor and marks the same
secondary indexes dirty **on every event**. Index dirtiness is an
idempotent boolean; component type high-water must equal that type's last
canonical cursor at each publication boundary.

## Proposed confined transformation

The journal is exclusively replayed by the invoker after worker join and
before any callback, index rebuild, deferred frontier, or user World
observation. Within a contiguous journal run of prevalidated events for
**the same component TypeId**:

- The World **MUST** advance the canonical `ChangeCursor` once, using the
  accepted checked epoch/tick transition, for **each** original event.
- Each event **MUST** still write its own exact row `changed_tick` at its
  accepted prevalidated storage address, in reference-ranked replay order.
- Only the **last event** in such a run publishes that component TypeId's
  final high-water cursor and marks its matching registered indexes dirty.
- A different TypeId, a fallback non-prevalidated event, a resource event,
  or the journal end **MUST** end that run before processing any later event.
  In particular, no type state persists across journals or cohorts.
- The direct World mutation path, resource path, fallback path, public
  API, system selection, mutable exposure admission, worker ownership, and
  memory-safety/structural-freeze proof remain untouched.

A one-element look-ahead makes publication at the last event explicit.
There is no buffered deferred summary, speculative cursor claim,
event dropping, skipped position, new global authority, or late cleanup
which a user panic might bypass. No arbitrary user callbacks occur inside
this private journal replay. The `World` event method still increments the
cursor for every event. The run-ending event publishes before the next
event is executed, so mixed-type/resource/fallback transitions retain
canonical high-water ordering.

## Proof and rejection criteria

- Focused tests verify independent row ticks for repeated and distinct
  rows, high-water changed-since threshold exactness, secondary-index
  rebuilding, mixed prevalidated/fallback/resource order, and epoch rollover.
- Existing C3 concurrent World projection, worker failure arbitration,
  near-terminal capacity exhaustion, reference-ranked replay, Runtime
  reuse, indexed queries and public serial equivalence suites MUST pass.
  Exact-head canonical Validation, Miri, AddressSanitizer and ThreadSanitizer
  are mandatory; accepted-main proofs remain independent.
- The existing **one** ABBA million-row benchmark lane uses a verified
  four-physical-core ARM64 runner (and records each run's actual `lscpu`).
  It measures exact `base/head/head/base` without changing the workload,
  with full serial, read-only, independent mutable and conflicting mutable
  cases at worker capacities 1, 2, 4, 8. All regression claims MUST come
  from this within-run paired evidence, not historical x64 averages.
- A material improvement in independent-writer *complete-schedule* time,
  no unjustified serial/read/conflict regressions, no changed observability,
  and manageable source complexity are **all required** for acceptance.
  A benchmark or prevalidated-only local win cannot override correctness.

This document specifies what must be proven; no projected speedup is
presented as an observed result.
