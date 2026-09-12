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
incompatibility, cycles, and deferred visibility, but the public API has no normalized
semantic diagnostic surface. Exposing the existing scheduler implementation types or
physical stages would recreate accidental public authority that RunenECS deliberately
removed.

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

### 4. Derive deferred visibility through a canonical serial reference and semantic publication frontiers

Ordinary ordering retains deferred-command visibility: deferred ECS work produced by a
semantic predecessor must be published before a semantic successor that requires that
visibility executes. The publication points that realize this are **semantic
publication frontiers**, not physical execution stages.

The deterministic serial reference sequence is itself derived from semantic schedule
facts rather than by flattening whatever physical grouping an executor happens to use.
For one valid semantic-precedence DAG, derive a private schedule-local precedence depth:

```text
depth(system with no semantic predecessors) = 0
depth(system) = 1 + max(depth(each semantic predecessor))
```

Then linearize systems by:

```text
(precedence depth, stable schedule-local registration/source ordinal)
```

The source ordinal is the stable internal order in which the built schedule received
its systems. It is a deterministic tie-break for otherwise same-depth systems; it does
not create a semantic precedence edge and is not portable identity. This linearization
preserves the accepted predecessor serial reference ordering while allowing future
physical stages/cohorts to split or combine work without redefining reference rank.

Precedence depth, source ordinal, and reference rank are private schedule-build
derivation facts. They must not be exposed as public stage identity, portable system
identity, or a promise that same-depth systems execute together. ADR 0003 consumes this
reference sequence and rank as its deterministic executor/publication tie-break; it does
not independently choose a different serial reference ordering.

Let the resulting deterministic serial reference sequence be:

```text
S0, S1, ... S(n-1)
```

A schedule-local **cut** `c` lies after every system with reference rank `< c` and
before the system with rank `c`; `c = n` is successful schedule completion.

A system is **deferred-producing** when its normalized parameter semantics permit it to
stage deferred ECS effects for runtime publication. This fact is separate from semantic
precedence, access incompatibility, physical executor grouping, and whether a concrete
runtime queue happens to be empty.

The parameter layer owns one normalized deferred-recorder classification. ADR 0003 may
distinguish local and transferable recorder classes for capability/validity purposes;
ADR 0001 consumes only the projection that any valid non-none recorder class is
deferred-producing. Local-versus-transferable capability does not itself create a
precedence edge or publication frontier, and `QueryAccess`, descriptor strings, runner
variants, and runtime queue existence are not parallel authorities for this fact.

Every direct reason-carrying semantic edge

```text
producer -> successor
```

whose producer is deferred-producing creates a publication obligation. At least one
frontier cut must satisfy:

```text
rank(producer) < cut <= rank(successor)
```

so the producer has completed and its deferred effects are committed before the
successor executes.

Every deferred-producing system also creates a successful-completion obligation with
deadline `n`. Any already-selected frontier after that producer satisfies the completion
obligation; schedule completion does not require a redundant terminal frontier when an
earlier publication has already covered the producer.

The canonical frontier sequence is the deterministic minimum-cardinality set of cuts
covering these interval obligations. Derive it with the earliest-deadline greedy rule:

1. order obligations by increasing deadline cut, using normalized semantic reason
   ordering only as a deterministic tie-break for diagnostics;
2. process each obligation `(producer_rank, deadline)` in that order;
3. if no selected frontier lies strictly after `producer_rank` and at or before the
   obligation deadline, select a frontier exactly at the deadline;
4. otherwise the existing frontier already satisfies the obligation.

Because all obligations are intervals on one deterministic serial reference sequence,
this greedy construction is canonical and minimum-cardinality for that sequence. An
implementation may use a different internal algorithm only when it is proven to produce
the same normalized reference and frontier sequences.

Frontiers are structural schedule facts, not queue-data events. If execution
successfully reaches a selected frontier, the runtime performs the publication and
invokes the corresponding frontier callback even when all buffers pending at that
frontier are empty. Callback cardinality and identity therefore remain independent of
per-run command production.

At a reached frontier, all successfully executed systems before the cut have completed,
pending deferred buffers for the publication interval are applied in accepted canonical
serial-reference and per-buffer order, publication is committed, and only then may the
frontier callback observe the World or execution continue beyond the cut. Future worker
execution must drain the relevant active cohort before this invoker-thread publication
and callback boundary.

A schedule with no deferred-producing systems has no deferred-publication frontier
merely to supply an application lifecycle tick. Conversely, a deferred-producing system
with no earlier covering frontier is guaranteed a completion frontier before successful
schedule return.

A frontier may incidentally publish deferred work from an otherwise unrelated system
that appears before the cut in the reference sequence. That does not create a new
precedence edge, ordering declaration, or portable pairwise visibility relation. It is a
consequence of this built schedule's canonical reference sequence and frontier cut.

Failure remains fail-stop. A system failure before an unreached frontier prevents that
frontier and callback; deferred-application failure or panic stops before its callback;
a callback error or panic occurs only after that frontier's publication is committed and
stops later execution. Earlier completed frontiers remain committed, later unpublished
deferred work is abandoned, and already-performed direct World mutations are not
implicitly rolled back.

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
- canonical semantic publication frontiers and their covered publication obligations;
- unordered access incompatibilities;
- pairwise concurrency assessment from schedule facts;
- absent optional ordering references.

A diagnostic system key is snapshot-local only. It exists to correlate facts inside one
inspection and may include a human-readable system name plus a same-name occurrence
number. It is not a runtime `SystemId`, is not accepted back by normal runtime mutation
or execution APIs, has no persistence/network meaning, and carries no execution-order
semantics. Equality across independently built inspection snapshots is not a stable
identity contract.

A publication-frontier key or ordinal is likewise schedule-build-local. It correlates
the canonical frontier facts inside one inspection/execution snapshot; it is not a
reference-rank leak, physical stage index, worker/cohort identity, portable persistence
identity, or public promise about executor grouping.

The inspection API must not publicly expose precedence depth, source/registration
ordinal, reference rank, internal `SystemId`, `SystemAccess`, `AccessKey`,
`AccessConflict`, raw graph node indices, stage indices, or physical executor groups.

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
**current pairwise schedule facts**.

A pair is prevented by one or both of:

- semantic precedence in either direction;
- access incompatibility.

If neither applies, the result is `unconstrained by ordering/access facts`, not
`guaranteed parallel` or `will run concurrently`. A semantic publication frontier is a
schedule-wide cut, not an independent pairwise precedence relation. Transferability,
invoking-thread requirements, frontier-crossing executor constraints, worker
availability, and physical executor policy belong to later threading/executor decisions.
Future capability facts may extend the assessment without changing precedence
semantics.

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
it does not create new execution semantics. Cycle diagnostics are defined before a
valid serial reference/frontier sequence exists and therefore must not depend on
reference rank to explain an invalid cyclic schedule.

### 9. Inspection is observational

Building or reading schedule diagnostics must not alter schedule order, insert
constraints, change deferred-command visibility, select different reference/frontier
sequences, run systems, or mutate the World.

The deterministic serial executor remains the correctness oracle. Valid schedules that
do not depend on formerly silent unresolved required references retain their accepted
semantic ordering behavior. Predecessor behavior that exposed physical stage callback
cardinality is intentionally not promoted to semantic compatibility.

## Consequences

RunenECS gains one semantic vocabulary for explaining schedules without promoting
executor layout into API. Missing required ordering targets become configuration errors,
while intentionally optional cross-plugin relations are visible and explicit. Access
ambiguities become inspectable without inventing order. Cycle errors become actionable
and deterministic.

Deferred publication now has one normalized schedule-derived meaning. The serial
reference sequence is derived from semantic precedence plus a private stable source
ordinal rather than future executor grouping. The callback sequence is then derived from
semantic visibility obligations over that reference sequence, not from stage/cohort
shape or runtime queue contents. Redundant visibility obligations share the same
canonical frontier, while every still-unpublished deferred producer is guaranteed
publication before successful schedule completion.

The cutover can require downstream source edits where existing `before` / `after`
relationships were intentionally optional or where consumers used physical deferred
callbacks as an application lifecycle clock. Those migrations are desirable: ordering
optionality belongs to the caller, while application/product publication policy belongs
to the downstream owner rather than RunenECS.

The public diagnostic surface is deliberately descriptive rather than executable.
Internal runtime identities, reference-rank derivation, access representations, and
physical execution groups remain implementation details.

This decision is a prerequisite for future transferable/thread-bound system capability
and parallel-executor work, but it does not authorize either capability. It also does
not authorize a raw stage/wave report, a generic scheduler framework, application/frame
or product-publication barriers, or an order-only deferred-visibility variant.
