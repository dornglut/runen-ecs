# ADR 0001: Normalize Schedule Diagnostics and Ordering References

> **Category: ADR**
>
> **Status:** Accepted
>
> **Decision date:** 2026-09-11

## Context

RunenECS owns explicit system ordering, system sets, schedule validation, access facts,
deferred-command boundaries, and deterministic serial reference execution.

The current scheduler keeps an important distinction already: `before` / `after` set
constraints create semantic precedence edges, while access conflicts do not create
ordering edges. The execution plan is then derived from the precedence graph.

Two gaps remain.

First, an ordering declaration that names a `SystemSetKey` with no matching *other*
system in the same schedule currently resolves to no edge and produces no diagnostic.
The caller cannot distinguish an intentionally optional cross-plugin relationship from
a misspelled, stale, or otherwise ineffective ordering requirement.

Second, the runtime has enough internal information to explain precedence, access
incompatibility, and cycles, but the public API has no normalized semantic diagnostic
surface. Exposing the existing scheduler implementation types or physical stages would
recreate accidental public authority that RunenECS deliberately removed.

Current Runenwerk consumers use system sets and cross-plugin `before` / `after`
relationships extensively. The design therefore has to make optionality explicit
without silently turning absent optional plugins into configuration failures.

## Decision

### 1. Keep four schedule concepts distinct

RunenECS treats these as separate facts:

```text
semantic precedence
access incompatibility
deferred visibility
physical executor grouping
```

Semantic precedence is the directed relation requested by explicit ordering
declarations. Access incompatibility is a symmetric fact derived from system access.
Deferred visibility states when deferred ECS mutations must be published relative to a
semantic ordering relation. Physical executor grouping is an implementation choice used
to realize those constraints.

Only semantic precedence creates ordering edges. Access incompatibility never creates
an implicit `A before B` or `B before A` relation. Physical stages, waves, batches,
worker assignments, scheduler indices, and registration-vector indices are not public
schedule semantics.

### 2. Make ordering-reference presence explicit

Every `before` / `after` declaration has a target-presence requirement.

The ordinary APIs are **required references**:

```text
system.before(TargetSet)
system.after(TargetSet)
```

A required declaration must resolve to at least one *other* system registered in the
same schedule and belonging to `TargetSet`. If it resolves to no such system, schedule
validation fails with a structured unresolved-ordering-reference error. A system's own
membership in the target set does not satisfy its declaration.

Intentional optional relationships use explicit APIs:

```text
system.before_if_present(TargetSet)
system.after_if_present(TargetSet)
```

An optional declaration that resolves to no other system creates no precedence edge and
is recorded as an absent-optional resolution in schedule inspection. If the target is
present, required and optional declarations have the same precedence and deferred-
visibility semantics.

Duplicate declarations are normalized by source system, direction, and target set. If
required and optional forms are both supplied for the same normalized declaration, the
required form dominates. The result must not depend on builder-call order.

This is a clean semantic cutover. Implementation must census current consumers and
migrate relationships that are intentionally optional to the explicit optional form;
RunenECS will not retain a compatibility mode where ordinary required declarations can
silently disappear.

### 3. Normalize ordering declarations into reason-carrying edges

A resolved declaration expands to one semantic edge for each matching target system:

```text
source before TargetSet  ->  source -> each matching target
source after TargetSet   ->  each matching target -> source
```

Each derived edge retains its declaration reason: source system, direction, target set,
reference requirement, predecessor, and successor. Multiple declarations may justify
the same edge; edge deduplication must not erase the ability to report a deterministic
semantic reason.

Transitive precedence is derived from this graph. It is semantic precedence, not a
physical execution batch.

### 4. Preserve deferred visibility as a separate consequence

The existing ordinary ordering contract retains deferred-command visibility across
semantic precedence: deferred ECS work produced by a predecessor must be published at
an accepted boundary before a semantic successor whose ordering requires that
visibility executes.

Diagnostics represent this as a deferred-visibility property of the ordering relation,
not by exposing the internal stage that happens to realize it. Unrelated systems may be
physically grouped around the same publication point, but that grouping does not become
portable ordering or public schedule meaning.

This ADR does not introduce an order-only or ignore-deferred ordering form. Such an API
would require a separate accepted decision because it changes observable command
visibility.

### 5. Expose a semantic inspection snapshot, not scheduler internals

RunenECS will expose an immutable `ScheduleInspection`-style snapshot for a valid built
schedule. The concrete Rust representation may optimize storage, but the public meaning
must contain these normalized facts:

- schedule identity;
- diagnostic system descriptors;
- normalized ordering declarations and their resolutions;
- reason-carrying resolved precedence edges;
- unordered access incompatibilities;
- pairwise concurrency assessment from schedule facts;
- absent optional ordering references.

A diagnostic system key is snapshot-local only. It exists to correlate facts inside one
inspection and may include a human-readable system name plus a same-name occurrence
number. It is not a runtime `SystemId`, is not accepted back by normal runtime mutation
or execution APIs, has no persistence/network meaning, and carries no execution-order
semantics. Equality across independently built inspection snapshots is not a stable
identity contract.

The inspection API must not publicly expose the internal `SystemId`, `SystemAccess`,
`AccessKey`, `AccessConflict`, raw graph node indices, stage indices, or physical
executor groups.

### 6. Report access ambiguities without resolving them

For every pair of systems in the same schedule, access incompatibility is derived from
ECS access facts independently of precedence.

An **access ambiguity** exists when:

1. the pair has at least one access conflict; and
2. neither system semantically precedes the other, including transitively.

The diagnostic records normalized reasons such as component/resource/world domain and
read/write versus write/write conflict. Diagnostic type names and labels are
explanatory data, not portable type identity.

If an access-conflicting pair is already ordered, the conflict remains a relevant
concurrency fact but is not reported as an ordering ambiguity. In neither case may the
conflict insert a precedence edge or change serial reference order.

### 7. Make concurrency assessment conservative and reason-carrying

Schedule inspection may answer whether two systems are prevented from overlapping by
**current schedule facts**.

A pair is prevented by one or both of:

- semantic precedence in either direction;
- access incompatibility.

If neither applies, the result is `unconstrained by ordering/access facts`, not
`guaranteed parallel` or `will run concurrently`. Transferability, invoking-thread
requirements, worker availability, and physical executor policy belong to later
threading/executor decisions. Future capability facts may extend the assessment without
changing precedence semantics.

### 8. Make cycle and unresolved-reference errors semantic and deterministic

Schedule validation gains a structured unresolved-ordering-reference failure containing
at least:

- schedule;
- source diagnostic descriptor;
- `before` or `after` direction;
- target set.

Ordering-cycle diagnostics contain a deterministic canonical cycle of semantic
ordering reasons rather than only `schedule has a cycle` and rather than a dump of
physical stages.

When several equivalent cycles or multiple declaration reasons for one edge exist, the
implementation must choose a stable canonical representation from normalized diagnostic
keys and declaration facts. The chosen representation is for deterministic diagnostics;
it does not create new execution semantics.

### 9. Inspection is observational

Building or reading schedule diagnostics must not alter schedule order, insert
constraints, change deferred-command visibility, run systems, or mutate the World.

The deterministic serial executor remains the correctness oracle. Valid schedules that
do not depend on formerly silent unresolved required references retain their existing
observable execution and deferred-visibility semantics.

## Consequences

RunenECS gains one semantic vocabulary for explaining schedules without promoting
executor layout into API. Missing required ordering targets become configuration errors,
while intentionally optional cross-plugin relations are visible and explicit. Access
ambiguities become inspectable without inventing order. Cycle errors become actionable
and deterministic.

The cutover can require downstream source edits where existing `before` / `after`
relationships were intentionally optional. That migration is desirable: optionality is
a caller decision and must not be inferred from an absent target.

The public diagnostic surface is deliberately descriptive rather than executable.
Internal runtime identities and access representations remain implementation details.

This decision is a prerequisite for future transferable/thread-bound system capability
and parallel-executor work, but it does not authorize either capability. It also does
not authorize a raw stage/wave report, a generic scheduler framework, application/frame
barriers, or an order-only deferred-visibility variant.
