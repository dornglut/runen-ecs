from pathlib import Path


def replace_exact(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one occurrence of {old!r}, found {count}")
    p.write_text(text.replace(old, new, 1))


adr2 = "docs/adr/0002-model-system-execution-mobility-as-a-proven-capability.md"
replace_exact(
    adr2,
    "| current ordinary `LocalCommands` | never transferable |",
    "| `LocalCommands` | never transferable |",
)
replace_exact(
    adr2,
    "The current runtime captures an `Rc<RefCell<...>>` deferred-command owner in\n"
    "every generated runner. That incidental capture must not survive in a runner\n"
    "classified transferable. Because current ordinary `LocalCommands` is itself\n"
    "invoker-thread-only, transfer-capable runners can be separated from that local\n"
    "deferred owner until a future transfer-safe deferred capability is accepted.",
    "At decision time, the serial runtime captured an `Rc<RefCell<...>>` local\n"
    "deferred-command owner in every generated runner. That incidental capture was\n"
    "not allowed to survive in a runner classified transferable. Subsequent accepted\n"
    "deferred-command work separated that local owner and introduced the transfer-safe\n"
    "recorder now named `Commands`. The invariant remains: a transferable runner must\n"
    "not capture `LocalCommands` or its local deferred owner.",
)
replace_exact(
    adr2,
    "Systems that use `WorldMut`, current `LocalCommands`, non-transfer-safe data, custom\n",
    "Systems that use `WorldMut`, `LocalCommands`, non-transfer-safe data, custom\n",
)
replace_exact(
    adr2,
    "This decision does not authorize a worker pool, parallel schedule execution,\n"
    "parallel query iteration, a transfer-safe deferred-command API, or application\n"
    "main-thread policy. Those remain separately bounded work.",
    "This decision by itself did not authorize a worker pool, parallel schedule\n"
    "execution, parallel query iteration, a transfer-safe deferred-command API, or\n"
    "application main-thread policy. Subsequent accepted deferred-command work\n"
    "separately introduced the transfer-safe recorder now named `Commands`; the other\n"
    "physical-execution and application-policy concerns remain separately bounded.",
)

adr3 = "docs/adr/0003-realize-deterministic-parallel-system-execution-from-serial-semantics.md"
replace_exact(
    adr3,
    "The present runtime is nevertheless serial-only. It invokes systems one at a time against `&mut World`, records successful `LocalCommands` into one runtime-local queue, advances World change cursors directly from mutable access, and invokes publication-frontier callbacks after the canonical semantic cuts derived from the schedule plan.",
    "At decision time, the runtime was serial-only. It invoked systems one at a time against `&mut World`, recorded deferred structural work in runtime-local command queues, advanced World change cursors directly from mutable access, and invoked publication-frontier callbacks after the canonical semantic cuts derived from the schedule plan.",
)
replace_exact(
    adr3,
    "- current ordinary `LocalCommands` is invoker-thread-only by ADR 0002;",
    "- the predecessor local recorder, now named `LocalCommands`, is invoker-thread-only by ADR 0002;",
)
replace_exact(
    adr3,
    "Current ordinary `LocalCommands` remains `InvokerThreadOnly` under ADR 0002. Its existing ability to queue arbitrary non-`Send` captured state is preserved; it is not silently tightened merely to enable workers.\n\nRunenECS introduces a distinct transferable deferred-command system parameter, semantically named `Commands`.",
    "The predecessor local recorder, now named `LocalCommands`, remains `InvokerThreadOnly` under ADR 0002. Its ability to queue arbitrary non-`Send` captured state is preserved; it is not silently tightened merely to enable workers.\n\nThis decision introduced a distinct transferable deferred-command system parameter, now named `Commands`.",
)
replace_exact(
    adr3,
    "It may contain multiple ordinary `LocalCommands` handles/fields, or multiple `Commands` handles/fields, provided same-class handles contribute to one logical per-system invocation buffer and preserve actual enqueue order. Ordinary `LocalCommands` and `Commands` must not coexist in one direct or nested system parameter graph.",
    "It may contain multiple `LocalCommands` handles/fields, or multiple `Commands` handles/fields, provided same-class handles contribute to one logical per-system invocation buffer and preserve actual enqueue order. `LocalCommands` and `Commands` must not coexist in one direct or nested system parameter graph.",
)
replace_exact(
    adr3,
    "`Commands` is a new capability, not a compatibility alias for `LocalCommands`.",
    "The transfer-safe `Commands` capability is distinct, not a compatibility alias for `LocalCommands`.",
)
replace_exact(
    adr3,
    "The finalized `Commands` buffer structurally preserves `Send` after erasure; ordinary `LocalCommands` buffers remain invoker-thread-only.",
    "The finalized `Commands` buffer structurally preserves `Send` after erasure; `LocalCommands` buffers remain invoker-thread-only.",
)
replace_exact(
    adr3,
    "The baseline executor drains the active worker cohort before invoking such a system and does not overlap worker execution across it. This is intentionally conservative and keeps thread-bound effects, whole-World access, and ordinary `LocalCommands` easy to reason about.",
    "The baseline executor drains the active worker cohort before invoking such a system and does not overlap worker execution across it. This is intentionally conservative and keeps thread-bound effects, whole-World access, and `LocalCommands` easy to reason about.",
)

adr4 = "docs/adr/0004-normalize-dense-storage-contiguity-and-expert-query-segments.md"
replace_exact(
    adr4,
    "ordinary query capabilities are valid only while structural mutation is frozen for the\ninvocation, `LocalCommands` publishes structural work later, and the exclusive `WorldMut`\n",
    "ordinary query capabilities are valid only while structural mutation is frozen for the\ninvocation, deferred command recorders publish structural work later, and the exclusive `WorldMut`\n",
)
