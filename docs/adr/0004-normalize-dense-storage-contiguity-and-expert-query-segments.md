# ADR 0004: Normalize Dense Storage Contiguity and Expert Query Segments

> **Category: ADR**
>
> **Status:** Accepted
>
> **Decision date:** 2026-09-12

## Context

RunenECS currently stores each dense archetype component column as `Vec<Box<T>>`
plus a parallel metadata vector. The outer vector is contiguous, but component payloads
are separately allocated. This gives each payload a stable heap address across vector
reallocation and archetype-container movement, but it prevents the storage layer from
truthfully exposing a contiguous typed component slice.

The current query runtime already relies on a stronger and more relevant lifetime rule:
ordinary query capabilities are valid only while structural mutation is frozen for the
invocation, deferred command recorders publish structural work later, and the exclusive `WorldMut`
parameter cannot coexist with sibling query capabilities. Direct `World`/`QueryState`
access is protected by ordinary Rust borrows. Component payload addresses therefore do
not need to remain stable across arbitrary future structural mutation; they need to
remain valid for the active borrow or invocation.

The current archetype registry also already has the right logical shape for columnar
storage. Every archetype has one entity row column, one component column for each
component type in its archetype key, and row-aligned added/changed metadata. Structural
moves use swap-removal and update the location of any entity moved into the removed
row. Retained component metadata is carried across archetype transitions.

The missing capability is not merely "make iteration faster." RunenECS needs a
normalized distinction between ordinary entity-wise query semantics and an expert
physical-layout capability that can prove same-length contiguous spans suitable for
bulk processing, auto-vectorization, external SIMD libraries, and later disjoint
parallel partitioning.

Current ecosystem practice supports this separation. Bevy ECS 0.19.1 uses columnar
table storage, models each table column as a type-erased `Vec<T>`, and exposes
contiguous iteration only for query data and filters that prove the required density;
otherwise contiguous iteration fails explicitly. Hecs and Flecs likewise expose
archetype/table-level homogeneous component columns rather than pretending an arbitrary
world query is one globally contiguous range. These are comparison evidence, not API
authority for RunenECS.

## Decision

### 1. Keep logical membership, physical layout, and execution policy distinct

RunenECS treats these as separate concepts:

```text
logical archetype membership
row alignment
physical contiguous segment
query membership/filtering
query iteration order
vectorized processing
parallel partitioning
```

An archetype states which component types an entity has. Row alignment states that the
entity at row `i`, every component value at row `i`, and every row-local metadata value
at row `i` refer to the same logical entity. A physical contiguous segment is a
temporary layout fact about some row interval. None of those facts creates a portable
query order, entity identity, persistence format, SIMD width, or worker partition.

Default query iteration order remains unspecified.

### 2. Dense component payloads become contiguous typed column storage

The dense/archetype storage direction is a typed contiguous value buffer equivalent in
meaning to `Vec<T>` for each component column segment, not a vector of independently
allocated `Box<T>` payloads.

The founding implementation target is one contiguous typed payload buffer per component
per archetype, with a row-aligned entity buffer and row-aligned component metadata.
The implementation may retain type erasure at the **column boundary** so the archetype
registry can own heterogeneous component columns. Type erasure must not require
per-component payload identity or a stable heap allocation for every row.

The concrete erased-column implementation may use a typed `Vec<T>` behind a sealed
column trait, a carefully proven type-erased contiguous buffer, or another
representation with the same invariants. Prefer the smallest representation with the
least unsafe memory-management surface. A raw blob allocator is not required merely
because another ECS uses one.

Components remain arbitrary supported Rust component values. This decision does not
require `Copy`, POD, zero drop glue, or a C-compatible representation. Structural moves
move Rust values and must drop every owned value exactly once.

Zero-sized components remain valid. Their slice length participates in row alignment,
but they do not gain a meaningful byte-stride or unique-address contract.

### 3. Contiguity is segment-local, not a promise that a whole query is one slice

A query can match multiple archetypes, so "contiguous query" does not mean that all
matching entities occupy one allocation.

The expert capability iterates **contiguous query segments**. Each yielded segment
covers one physically contiguous row interval in one matching storage segment. For
query data such as:

```text
&T
&mut T
(Entity, &T)
(&A, &B)
(&mut A, &B)
(&mut A, &mut B)
```

a segment yields corresponding same-length typed spans, conceptually:

```text
&[T]
&mut [T]
(&[Entity], &[T])
(&[A], &[B])
(&mut [A], &[B])
(&mut [A], &mut [B])
```

The exact Rust spelling may use sealed query-data traits and lightweight wrapper types,
especially for mutable spans that must preserve change tracking. The semantic contract
is the same-length row-aligned span relation, not a particular tuple/wrapper name.

The first implementation may yield one segment per matching archetype. The public
contract must not make that segmentation stable: a future accepted storage
implementation may split an archetype into several physical chunks and yield more
segments without changing query membership semantics.

No segment index, pointer, base address, row number, capacity, or chunk boundary is a
portable identity or ordering key.

### 4. Payload relocation is legal only outside active reference/freeze scopes

Contiguous value buffers may reallocate or move payloads when structural mutation is
allowed. RunenECS therefore does **not** promise stable component addresses across:

- spawn/despawn;
- component insertion/removal;
- archetype migration;
- storage compaction/reservation;
- a completed system/query invocation followed by later structural work.

References and slices remain valid for their normal Rust/query lifetimes because
structural mutation is forbidden for that scope:

- shared/direct world borrows prevent mutable structural access;
- mutable direct world/query borrows prevent competing structural access;
- ordinary system query capabilities exist under the runtime structural freeze;
- deferred commands do not apply until the accepted publication boundary;
- exclusive whole-World access is not combined with sibling query capabilities.

Any internal raw pointer, slice descriptor, or projected column reference used by the
query runtime must be invocation/borrow scoped and must not be cached across a
structural mutation boundary.

This replaces the current accidental "stable because every payload is boxed" property
with the actual semantic invariant: **stable for the active access scope, movable
between structural epochs**.

### 5. Preserve one row-alignment invariant through every structural operation

For every physical archetype segment with `N` rows:

```text
entities.len() == N
for every component column C: C.values.len() == N
for every component column C: C.metadata.len() == N
```

At row `i`, all three domains describe the same entity.

Archetype transitions must preserve this invariant as one operation. For recoverable
validation failures, current operation-level structural atomicity remains: preflight
must reject the operation before visible partial mutation.

For a successful move:

- retained component values move to the destination row;
- retained component `added` and `changed` metadata move with those values unchanged;
- newly inserted components receive the insertion change position according to the
  existing change-observation contract;
- removed components are returned/dropped according to the existing structural API;
- source-row swap removal is mirrored for the entity row and every source component
  column;
- if another entity is swapped into the vacated source row, its `EntityLocation` is
  updated before the operation is externally observable;
- destination `EntityLocation` is published only for the complete aligned row.

Physical row order may change after any structural operation. No API may treat current
row position as stable entity order.

This ADR does not require transactional rollback after an arbitrary user destructor
panic. It does require memory safety, no double drop, and preservation of the existing
preflight/operation-level failure semantics for recoverable ECS errors.

### 6. Contiguous query support is a sealed, proven capability

Not every `QueryData` / `QueryFilter` shape is contiguous-segment capable.

RunenECS will add a framework-owned sealed proof classification conceptually equivalent
to:

```text
ContiguousQueryData
ArchetypeOnlyQueryFilter
```

Exact internal/public trait names are not fixed by this ADR. Downstream safe code cannot
forge the proof.

The founding supported query-data family is deliberately conservative:

- `Entity` where paired with component spans;
- required shared component access `&T`;
- required mutable component access `&mut T`;
- tuples of distinct required shared/mutable component accesses for which ordinary query
  access validation already proves non-aliasing.

The founding filter family is limited to filters whose truth is uniform for an entire
archetype/segment, including:

- no filter;
- `With<T>`;
- `Without<T>`;
- tuples composed only from such archetype-level filters.

Row-selective filters such as `Added<T>` and `Changed<T>` are not contiguous in the
founding capability because they can create holes inside a storage segment.

Optional component query forms are also excluded initially. An archetype makes an
optional component all-present or all-absent for that segment, so an extension is
possible later, but it should be added only with a clear segment-level result shape and
real consumer pressure rather than complicating the founding proof.

### 7. Failure is explicit; contiguous APIs never silently become scalar iteration

Requesting contiguous segments from a query shape that cannot prove them returns a
structured/focused failure such as `QueryNotContiguous`; it does not silently call the
ordinary entity iterator.

A caller that accepts scalar fallback writes that policy explicitly:

```text
try contiguous expert path
else ordinary query iteration
```

This keeps optimization claims truthful. Code can know whether it actually received the
stronger physical capability.

The failure vocabulary should explain the semantic class of failure without exposing
raw storage pointers, archetype-vector indices, allocation addresses, or other internal
layout identities.

### 8. Mutable spans preserve existing change-observation semantics

A mutable contiguous segment is still a mutable query access. Yielding it must preserve
the current conservative change-observation rule for every row/component that receives
mutable access.

The founding implementation must therefore mark each affected mutable component row as
changed before or as the mutable span becomes observable, and must preserve:

- which component types are marked by mixed mutable/shared tuples;
- `Added` metadata for retained components;
- `Changed` observations made by later queries;
- component-index invalidation semantics;
- World-scoped change-cursor lineage.

Contiguous iteration must not gain performance by silently weakening change tracking.
A later accepted change may batch equivalent bookkeeping only after proving that the
public change-observation contract, cursor behavior, index invalidation, and diagnostics
are observationally equivalent.

The exact public mutable-segment type may wrap `&mut [T]` so bookkeeping is performed by
RunenECS before safe mutable slice access is exposed. No bypass-change-detection API is
authorized by this decision.

### 9. Reflection remains typed at the owning access boundary

Reflection and dynamic inspection do not require a public untyped contiguous storage
API.

Existing reflected component access may continue to locate an entity row and invoke the
registered type-specific reflection adapter for that component. The storage registry
may use internal type erasure to find the correct column, but it must re-establish the
registered concrete type before manufacturing a Rust reference.

Do not expose raw byte slices, `&[dyn Reflect]` fabricated from heterogeneous storage,
or public column downcasts merely to advertise contiguity. The expert contiguous query
capability is statically typed and sealed.

### 10. SIMD width, allocation alignment, and chunk size are not ECS semantics

The contiguous capability guarantees valid Rust typed slices/spans and row alignment.
It does not guarantee:

- 16/32/64-byte SIMD alignment beyond the alignment required for `T`;
- a particular CPU vector width;
- a fixed minimum/maximum segment length;
- a cache-line or page boundary;
- stable archetype/chunk packing;
- use of explicit SIMD instructions.

Consumers may use ordinary slice processing, compiler auto-vectorization, portable SIMD
when available in their toolchain, or external SIMD libraries. Any stronger alignment
or width contract requires separate evidence and an accepted API decision.

### 11. Future parallel query partitioning may split spans, not semantic order

A later parallel-query capability may subdivide a proven contiguous segment into
non-overlapping row ranges and process those ranges concurrently when access and
execution-mobility proofs permit it.

Such partitioning is a physical execution realization. It must not make segment order,
row order, worker assignment, split size, or completion order semantic. Results that
need deterministic reduction/commit order must define that order separately rather than
using current storage addresses as authority.

This ADR does not implement parallel query execution and does not modify ADR 0003's
system-level deterministic parallel execution rules.

### 12. Benchmark evidence gates optimization claims, not the semantic storage law

Issue #17 owns correction of the semantic benchmark fixtures. No storage implementation
may claim improvement or set regression thresholds against the misleading predecessor
transition/deferred numbers.

The semantic decision in this ADR does not depend on proving that contiguous storage is
faster on one machine. It removes an accidental per-row allocation requirement and
creates an honest stronger capability. Implementation acceptance still requires the
corrected #17 baseline so storage-transition/query effects can be measured against
truthful workloads.

The first implementation must separate at least:

```text
storage representation / archetype transition cost
ordinary scalar query cost
contiguous-segment query cost
scheduler/system execution cost
```

Do not add SIMD intrinsics in the storage cutover merely to manufacture a performance
win.

## Consequences

RunenECS can remove per-component payload boxes from dense archetype storage without
weakening reference safety because address stability is normalized to the actual
structural-freeze lifetime. Dense columns become capable of yielding true typed
contiguous spans. Entity/value/metadata row alignment becomes the central storage
invariant rather than stable payload addresses.

The expert query surface is intentionally narrower than ordinary querying. It provides
a strong physical fact when provable and fails explicitly otherwise. Ordinary queries
retain their current semantics and remain the universal fallback.

Mutable bulk access initially still incurs truthful per-row change bookkeeping. That may
limit some speedups, but weakening `Changed`/index semantics is not an acceptable hidden
optimization. Future bookkeeping optimization must prove equivalence separately.

The segment-oriented contract leaves room for future chunked/segmented storage and
parallel range partitioning without promising a stable chunk topology today.

## Rejected alternatives

### Keep `Vec<Box<T>>` and add a "contiguous" iterator façade

Rejected. Contiguous pointers are not contiguous payloads; presenting them as a SIMD
component span would be false.

### Require one globally contiguous slice per query

Rejected. Queries can match several archetypes and future storage may segment one
archetype. Global contiguity would either copy data or turn a physical accident into a
portable contract.

### Adopt fixed-size chunks as public architecture immediately

Rejected. Chunking may become useful for allocator behavior, structural churn, or
parallel partitioning, but no corrected benchmark currently justifies fixing a chunk
size/topology. Segment-oriented API semantics preserve that future option.

### Expose raw type-erased bytes for dynamic SIMD/reflection

Rejected. This widens unsafe layout authority, complicates drop/alignment semantics, and
is unnecessary for the typed expert capability.

### Silently fall back to scalar iteration

Rejected. The caller would be unable to know whether the requested stronger capability
was actually realized, undermining performance evidence and diagnostics.

### Batch mutable change positions immediately

Rejected for the founding cut. It may be observationally equivalent under a refined
contract, but current code advances/records change positions during mutable query fetch.
Storage contiguity does not authorize a change-observation redesign.

## Implementation sequencing

This ADR accepts the storage/query direction only. It does not authorize publication of
uncompiled Rust changes.

After #17 supplies a corrected benchmark oracle and a checked-out executor is available,
implementation should be split so unsafe proof changes remain reviewable. A likely
sequence is:

1. replace dense payload boxing with contiguous typed column storage while preserving
   ordinary scalar query behavior and all structural/reflection/index/change tests;
2. add the sealed contiguous-segment query capability and focused failure semantics;
3. measure ordinary versus contiguous workloads using the corrected benchmark baseline;
4. consider later segment/chunk, bookkeeping, SIMD, or parallel-query optimization only
   from demonstrated pressure.

Each implementation slice must re-derive the affected unsafe proof and run canonical
validation plus applicable Miri/AddressSanitizer evidence at the unchanged reviewed
head.

## External comparison references

- Bevy ECS 0.19.1 `Table`: column-oriented storage whose columns are type-erased
  `Vec<T>` values.
- Bevy ECS 0.19.1 `Query::contiguous_iter` / `contiguous_iter_mut`: explicit fallible
  contiguous iteration for proven dense query/filter forms.
- hecs 0.11 `Archetype::get`: typed column access scoped to one archetype.
- Flecs query iteration: table-by-table component arrays with same-row component pairing.
