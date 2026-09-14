# ADR 0002: Model System Execution Mobility as a Proven Capability

> **Category: ADR**
>
> **Status:** Accepted
>
> **Decision date:** 2026-09-11

> **Current command terminology (2026-09-14):** Later accepted deferred-command
> work added the transfer-safe recorder, and issue #69 normalized public names so
> `Commands` is that normal transfer-safe capability while `LocalCommands` is the
> explicit invoker-thread-only capability. This updates terminology only; the
> mobility proof and local-capability decision below are unchanged.

## Context

RunenECS intentionally permits `Component` and `Resource` types that are only
`'static`. The reusable ECS contract does not globally require `Send + Sync`.
That property is useful for owner-thread state and must survive future parallel
execution work.

The current runtime is a deterministic serial reference executor. Its physical
representation is invoker-thread-local:

- registered systems erase to `Box<dyn FnMut(&mut World) -> ...>` without a
  `Send` bound;
- system parameter state may itself be non-`Send`;
- `QueryState` currently uses `Rc<RefCell<...>>` scratch ownership;
- the runtime deferred-command owner uses `Rc<RefCell<...>>`;
- `LocalCommands::queue` accepts arbitrary `'static` closures without a `Send`
  bound, so a queued effect may capture thread-bound state;
- `WorldMut` exposes the complete World, which may contain `!Send` / `!Sync`
  data.

Those facts are valid for the serial baseline. They are not a safe basis for
moving a system invocation to a worker thread.

A future parallel executor therefore needs a capability model that proves when
one system invocation may move away from the thread calling `run_schedule`
without turning hypothetical parallelism into global data-trait bounds or
scheduler semantics.

Current ecosystem designs confirm the underlying distinction but do not define
RunenECS policy. Bevy marks systems that use non-send data for main-thread
execution, while Shipyard exposes explicit thread-local access forms for
`!Send` / `!Sync` data. RunenECS instead keeps its host-neutral invoker-thread
model and derives transfer safety from the exact callable and parameter facts.

## Decision

### 1. Execution mobility is a separate ECS fact

RunenECS defines one system execution-mobility capability with two states:

```text
Transferable
InvokerThreadOnly
```

`Transferable` means RunenECS holds a compile-time proof that the registered
system callable, cached parameter state, and immediate ECS access contract may
be moved as one invocation from the schedule-invoking thread to another thread.

`InvokerThreadOnly` means the invocation must execute on the thread that called
the schedule for that invocation.

The invoker thread is not a framework-defined "main thread". A World and its
serial schedule may be owned and invoked from any thread allowed by Rust; this
capability only constrains whether the runtime may move an invocation elsewhere.

Execution mobility is distinct from:

```text
semantic precedence
access incompatibility
deferred visibility
physical executor grouping
```

A transferable system is not thereby parallel, concurrently eligible, or
promised worker execution. A future executor must still satisfy precedence,
access, deferred visibility, cancellation, publication, and worker-policy
constraints.

### 2. Normal system registration means proven transferable

The normal/raw system-registration path is the transferable path.

A normal function or closure may be registered without an invoker-thread
wrapper only when the framework can prove all of the following:

1. the callable/captured state is `Send + 'static`;
2. every system parameter has a transferable parameter proof;
3. every cached parameter state that moves with the registered system is
   `Send`;
4. the parameter set does not expose whole-World access or another
   invoker-thread-only effect;
5. the transferable proof survives type erasure in the registered system.

`Sync` is not required for the callable merely because it is transferable. A
registered system instance is uniquely owned and mutably invoked; transfer
requires `Send`, not concurrent shared invocation.

A system that does not or must not satisfy those conditions is made explicit
with an invoker-thread configuration wrapper, semantically:

```text
system.on_invoker_thread()
```

The concrete extension-trait plumbing may change during implementation, but the
public contract is fixed: ordinary registration is the proven-transferable
path, and invoker-thread execution is an explicit system-level capability.

The wrapper is allowed even when a system could otherwise be transferable. In
that case the explicit thread-affinity decision wins. This is a capability
restriction, not an ordering edge.

This is a clean cutover. RunenECS does not keep a permissive registration path
that silently erases whether transfer safety was proven.

### 3. Core data traits remain unconstrained

`Component` and `Resource` remain `'static` contracts. They do not gain global
`Send`, `Sync`, or `Send + Sync` supertraits.

Thread safety is proved at the access site instead.

For an immediate shared access to payload `T` on another thread, `T: Sync` is
required. For an immediate exclusive mutable access to payload `T` on another
thread, `T: Send` is required. These are intentionally different requirements:
RunenECS does not require a type to be both `Send` and `Sync` when the actual
access mode needs only one property.

This permits, for example, an exclusively accessed `T: Send + !Sync` component
to participate in a transferable system while a shared access to the same type
remains invoker-thread-only.

### 4. Transferable parameter proof is type-level and unsafe to forge

RunenECS introduces a framework-owned low-level transferable-parameter proof,
conceptually:

```text
unsafe TransferableSystemParam: SystemParam
```

The exact internal name is not semantic API, but the contract is.

A transferable parameter proof guarantees at least:

- its cached `State` can move with the registered system;
- its extraction/execution may occur away from the invoker thread under the
  validated access contract;
- every payload access satisfies the required Rust thread-safety property;
- it does not reconstruct unrestricted whole-World access;
- it does not hide an invoker-thread-only deferred effect;
- its implementation remains valid under future disjoint World capability
  projection rather than relying on the current serial `&mut World` call site.

Safe downstream code cannot assert this proof. Framework-provided parameter
forms receive implementations only where their bounds prove the contract.
Manual low-level implementations, where supported at all, require `unsafe` and
therefore own the corresponding safety proof.

Runtime metadata may describe the resulting capability, but a runtime boolean
or access report is never the safety proof.

### 5. Built-in parameter classification follows actual accessed data

The transferable proof for built-in parameters follows these rules.

| Parameter/access form | Transferable condition |
| --- | --- |
| Entity-only / ECS metadata-only query data | transfer-safe cached state |
| shared component access `&T` | `T: Component + Sync` |
| mutable component access `&mut T` | `T: Component + Send` |
| optional / tuple query data | every yielded child access satisfies its rule |
| metadata-only filters such as membership/change metadata | no extra payload `Send`/`Sync` bound beyond the data actually touched |
| `Res<T>` | `T: Resource + Sync` |
| `ResMut<T>` | `T: Resource + Send` |
| `RemovedQuery<T>` | transfer-safe cached state when only removal metadata is read |
| `WorldMut` | never transferable |
| current ordinary `LocalCommands` | never transferable |

The table describes semantic proof requirements, not the current storage
implementation. If incidental runtime state is non-`Send`, implementation must
repair that representation before granting the transferable proof; it must not
label the parameter transferable and hope a future executor makes the claim
true.

In particular, the current `QueryState` scratch pools use `Rc<RefCell<...>>`.
That representation prevents the state from moving even when the query payload
bounds are otherwise sufficient. The implementation may replace incidental
shared scratch ownership with uniquely owned / transfer-safe cached state while
preserving query semantics, or keep that query shape invoker-thread-only until
such a proof exists.

### 6. Composite and derived parameters are conservative meets

Tuple parameters and `#[derive(SystemParam)]` groups are transferable exactly
when every child parameter is transferable and the composed cached state is
transferable.

The derive must generate the transferable proof only under child-proof bounds.
It must not copy a runtime flag from user data or provide an opt-out that can
forge transferability in safe code.

One invoker-thread-only child makes the whole system parameter group
invoker-thread-only unless that child is replaced by a separately accepted
transfer-safe capability.

### 7. Whole-World access is invoker-thread-only

`WorldMut` remains a valid serial/invoker-thread parameter and retains its
exclusive-world access semantics.

It is not transferable because RunenECS deliberately permits the World to
contain data that cannot move or be shared across threads. Exclusive access to
"everything" cannot honestly prove that all contained state is worker-safe
without reintroducing a global World/data bound.

`WorldMut` therefore contributes two independent facts:

```text
exclusive ECS access
InvokerThreadOnly execution mobility
```

The first prevents conflicting concurrent World access. The second fixes the
invocation to the invoker thread. Neither fact should be inferred from the
other in the general model.

### 8. Deferred command capabilities preserve mobility explicitly

`Commands` is the normal transfer-safe deferred recorder. Its queued effects carry the
required transfer bound, and normal systems using it may satisfy the transferable proof
when their other callable/parameter facts do as well.

`LocalCommands` is the explicit invoker-thread-only recorder. It continues to accept
arbitrary `'static` queued closures that may capture `!Send` state, so it cannot
participate in the transferable proof and must be used through explicit
`system.on_invoker_thread()` registration.

Both capabilities retain the same semantic deferred-mutation model: ordered fail-stop
application and schedule-derived publication visibility. The distinction is execution
mobility of the deferred work, not whether structural mutation is conceptually local.
### 9. Callable and parameter proofs must survive type erasure

The registered-system representation must preserve the distinction structurally.
It is not sufficient to store an `is_transferable = true` flag beside one
non-`Send` erased runner.

A transferable registered system must retain an erased callable/state
representation that itself satisfies the movement proof, for example through a
`Send`-bounded erased runner variant or an equivalent proof-preserving design.
An invoker-thread-only runner remains non-transferable and cannot be recovered
through a safe API as a transferable runner.

The current runtime captures an `Rc<RefCell<...>>` deferred-command owner in
every generated runner. That incidental capture must not survive in a runner
classified transferable. Because current ordinary `LocalCommands` is itself
invoker-thread-only, transfer-capable runners can be separated from that local
deferred owner until a future transfer-safe deferred capability is accepted.

This requirement exists so the future parallel executor can rely on the
accepted capability rather than re-auditing a lossy boolean classification.

### 10. Serial execution remains the behavioral oracle

The serial reference executor runs both capability classes on the schedule
invoking thread in the same semantic order and with the same deferred-visibility
rules.

Adding mobility classification alone does not change:

- system ordering;
- access validation;
- query iteration semantics;
- change observation;
- deferred-command publication;
- error or panic behavior.

`Transferable` is permission for a future physical executor to move an
invocation; the serial executor need not exercise that permission.

### 11. Diagnostics expose capability, not worker policy

RunenECS schedule/system diagnostics expose the normalized execution-mobility
fact:

```text
Transferable
InvokerThreadOnly
```

For invoker-thread systems, diagnostics report an owner-neutral reason such as
an explicit invoker-thread wrapper or a framework-known thread-bound parameter
capability. Diagnostics may identify the relevant parameter slot using the
normalized parameter descriptor model.

Diagnostics do not expose worker IDs, thread-pool topology, task batches, or a
promise that a transferable system will execute off-thread.

ADR 0001's schedule inspection may incorporate these mobility facts, but its
precedence/access model remains unchanged. Mobility does not create ordering
edges or resolve access ambiguities.

### 12. Parallel eligibility is a later conjunction

A future executor may consider an invocation for worker execution only when at
least all of these are true:

```text
system is Transferable
AND semantic precedence permits it
AND ECS access is compatible with concurrent work
AND deferred/publication constraints permit it
AND executor capacity/policy chooses it
```

ADR 0002 defines only the first term. ADR 0001 owns semantic scheduling
explanation; the future parallel-executor decision owns the physical
realization.

## Consequences

RunenECS keeps `!Send` / `!Sync` ECS state as a first-class serial capability
without imposing global thread-safety bounds. Transfer safety becomes an
explicit proof attached to the exact access mode that needs it.

Normal system registration becomes future-parallel-ready by construction.
Systems that use `WorldMut`, current `LocalCommands`, non-transfer-safe data, custom
thread-bound state, or non-`Send` closure captures remain usable through an
explicit invoker-thread wrapper. Existing consumers may therefore require a
source migration when this design is implemented; that migration is preferable
to retaining ambiguous registration semantics.

The design also exposes current implementation blockers honestly: query cached
state and deferred command ownership contain `Rc`-based structures that must not
be hidden behind a transferable metadata flag. Any implementation must preserve
the proof through type erasure before a parallel executor can consume it.

This decision does not authorize a worker pool, parallel schedule execution,
parallel query iteration, a transfer-safe deferred-command API, or application
main-thread policy. Those remain separately bounded work.
