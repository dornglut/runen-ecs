from pathlib import Path
import re
import subprocess


def git_mv(old: str, new: str) -> None:
    subprocess.run(["git", "mv", old, new], check=True)


# Keep internal source layout aligned with the public default/local vocabulary.
git_mv("crates/runen-ecs/src/commands/batch.rs", "crates/runen-ecs/src/commands/local_batch.rs")
git_mv("crates/runen-ecs/src/commands/command_buffer.rs", "crates/runen-ecs/src/commands/local.rs")
git_mv("crates/runen-ecs/src/commands/transferable.rs", "crates/runen-ecs/src/commands/commands.rs")
git_mv("crates/runen-ecs/src/commands/transferable_batch.rs", "crates/runen-ecs/src/commands/batch.rs")
git_mv("crates/runen-ecs/tests/transferable_commands.rs", "crates/runen-ecs/tests/commands.rs")
git_mv("crates/runen-ecs/tests/ui/transferable_commands_non_send.rs", "crates/runen-ecs/tests/ui/commands_non_send.rs")
git_mv("crates/runen-ecs/tests/ui/transferable_commands_non_send.stderr", "crates/runen-ecs/tests/ui/commands_non_send.stderr")
git_mv("crates/runen-ecs/tests/ui/transferable_spawn_non_send.rs", "crates/runen-ecs/tests/ui/commands_spawn_non_send.rs")
git_mv("crates/runen-ecs/tests/ui/transferable_spawn_non_send.stderr", "crates/runen-ecs/tests/ui/commands_spawn_non_send.stderr")

# Existing direct World::commands() uses were written against the predecessor local
# capability. Preserve those uses explicitly before changing the default entrypoint.
for root in [Path("crates/runen-ecs/tests"), Path("crates/runen-ecs/examples"), Path("conformance")]:
    if not root.exists():
        continue
    for path in root.rglob("*.rs"):
        text = path.read_text()
        updated = re.sub(r"\bworld\.commands\(\)", "world.local_commands()", text)
        if updated != text:
            path.write_text(updated)

# Clean public symbol cut. Replace predecessor local names first.
replacements = [
    (r"\bBatchCommands\b", "LocalBatchCommands"),
    (r"\bCommands\b", "LocalCommands"),
    (r"\bTransferableBatchCommands\b", "BatchCommands"),
    (r"\bTransferableCommands\b", "Commands"),
]
tracked = subprocess.check_output(["git", "ls-files"], text=True).splitlines()
for name in tracked:
    path = Path(name)
    if path.suffix not in {".rs", ".md", ".stderr"} or not path.exists():
        continue
    text = path.read_text()
    updated = text
    for pattern, replacement in replacements:
        updated = re.sub(pattern, replacement, updated)
    if updated != text:
        path.write_text(updated)

Path("crates/runen-ecs/src/commands/mod.rs").write_text(
    "mod batch;\n"
    "mod commands;\n"
    "mod deferred;\n"
    "mod local;\n"
    "mod local_batch;\n"
    "mod queue;\n"
    "mod transferable_buffer;\n\n"
    "pub use batch::BatchCommands;\n"
    "pub use commands::Commands;\n"
    "pub use local::LocalCommands;\n"
    "pub use local_batch::LocalBatchCommands;\n\n"
    "pub(crate) use transferable_buffer::TransferableCommandBuffer;\n"
)

local_path = Path("crates/runen-ecs/src/commands/local.rs")
text = local_path.read_text()
text = text.replace("use super::batch::LocalBatchCommands;", "use super::local_batch::LocalBatchCommands;")
text = text.replace(
    "pub struct LocalCommands<'world> {",
    "/// Explicit invoker-thread-local recorder for deferred structural effects.\n"
    "///\n"
    "/// Unlike [`crate::Commands`], this capability may queue arbitrary `!Send`\n"
    "/// work. Systems using it must therefore opt into `.on_invoker_thread()`.\n"
    "pub struct LocalCommands<'world> {",
)
text = text.replace(
    '"commands param escaped its system execution scope"',
    '"local commands param escaped its system execution scope"',
)
local_path.write_text(text)

commands_path = Path("crates/runen-ecs/src/commands/commands.rs")
text = commands_path.read_text()
text = text.replace("use super::transferable_batch::BatchCommands;", "use super::batch::BatchCommands;")
text = text.replace(
    "/// Invocation-local recorder for transfer-safe deferred structural effects.\n",
    "/// Default invocation-local recorder for transfer-safe deferred structural effects.\n",
)
text = text.replace(
    '"transferable commands param escaped its system execution scope"',
    '"commands param escaped its system execution scope"',
)
text = text.replace(
    '"external transferable command owner finalization requires runtime command owner"',
    '"external command owner finalization requires runtime command owner"',
)
commands_path.write_text(text)

local_batch_path = Path("crates/runen-ecs/src/commands/local_batch.rs")
text = local_batch_path.read_text().replace(
    "/// Ordered group of deferred commands.",
    "/// Ordered group of invoker-thread-local deferred commands.",
)
local_batch_path.write_text(text)

# Root exposes both capabilities; prelude exposes only the normal default path.
lib_path = Path("crates/runen-ecs/src/lib.rs")
text = lib_path.read_text()
text = re.sub(
    r"pub use commands::\{[^\n]+\};",
    "pub use commands::{BatchCommands, Commands, LocalBatchCommands, LocalCommands};",
    text,
    count=1,
)
lib_path.write_text(text)

prelude_path = Path("crates/runen-ecs/src/prelude.rs")
text = prelude_path.read_text()
text = re.sub(r"\bLocalBatchCommands,\s*", "", text)
text = re.sub(r"\bLocalCommands,\s*", "", text)
prelude_path.write_text(text)

# World default is transfer-safe; arbitrary local work is explicit.
world_runtime = Path("crates/runen-ecs/src/world/runtime.rs")
text = world_runtime.read_text()
text = text.replace("use crate::commands::LocalCommands;", "use crate::commands::{Commands, LocalCommands};")
old = "    pub fn commands(&self) -> LocalCommands<'static> {\n        LocalCommands::new()\n    }\n"
new = (
    "    pub fn commands(&self) -> Commands<'static> {\n"
    "        Commands::new()\n"
    "    }\n\n"
    "    pub fn local_commands(&self) -> LocalCommands<'static> {\n"
    "        LocalCommands::new()\n"
    "    }\n"
)
if old not in text:
    raise SystemExit("expected World::commands predecessor shape not found")
world_runtime.write_text(text.replace(old, new))

# User-facing parameter descriptors use current names.
params_path = Path("crates/runen-ecs/src/system/params.rs")
text = params_path.read_text()
text = text.replace(
    'ParamSlotDescriptor::leaf("commands", "LocalCommands", std::any::type_name::<Self>())',
    'ParamSlotDescriptor::leaf("local_commands", "LocalCommands", std::any::type_name::<Self>())',
)
text = text.replace(
    '"transferable_commands",\n            "Commands",',
    '"commands",\n            "Commands",',
)
params_path.write_text(text)

# Files using prelude plus the exceptional local type must import it explicitly.
for root in [Path("crates/runen-ecs/examples"), Path("crates/runen-ecs/tests"), Path("conformance")]:
    if not root.exists():
        continue
    for path in root.rglob("*.rs"):
        text = path.read_text()
        if "LocalCommands" not in text or "use runen_ecs::prelude::*;" not in text:
            continue
        if "use runen_ecs::LocalCommands;" in text or "runen_ecs::LocalCommands" in text:
            continue
        text = text.replace(
            "use runen_ecs::prelude::*;\n",
            "use runen_ecs::prelude::*;\nuse runen_ecs::LocalCommands;\n",
            1,
        )
        path.write_text(text)

# Teach the explicit local recorder in the mobility example.
Path("crates/runen-ecs/examples/system_mobility.rs").write_text(
'''use runen_ecs::LocalCommands;
use runen_ecs::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Copy, Clone)]
struct Update;
impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Debug, runen_ecs::Resource)]
struct Frame(u32);

fn advance(mut frame: ResMut<Frame>) {
    frame.0 += 1;
}

fn main() {
    let mut world = World::new();
    world.insert_resource(Frame(0));

    let local_ran = Rc::new(Cell::new(false));
    let captured = Rc::clone(&local_ran);

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, advance);
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (move |mut commands: LocalCommands| {
            let deferred_capture = Rc::clone(&captured);
            commands.queue(move |_world| {
                deferred_capture.set(true);
                Ok(())
            });
        })
        .on_invoker_thread(),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert_eq!(world.resource::<Frame>().unwrap().0, 1);
    assert!(local_ran.get());

    // Normal registration means proven transferable eligibility. LocalCommands
    // can carry !Send deferred work, so that capability is imported explicitly
    // and the system is restricted to the thread that invokes the schedule.
    println!("default transferable system and explicit local-command system both ran");
}
''')

# Renamed trybuild fixtures and direct normal-registration rejection for LocalCommands.
mobility_test = Path("crates/runen-ecs/tests/system_param_mobility.rs")
text = mobility_test.read_text()
text = text.replace('tests/ui/transferable_commands_non_send.rs', 'tests/ui/commands_non_send.rs')
text = text.replace('tests/ui/transferable_spawn_non_send.rs', 'tests/ui/commands_spawn_non_send.rs')
mobility_test.write_text(text)

Path("crates/runen-ecs/tests/ui/mobility_commands.rs").write_text(
'''use runen_ecs::{LocalCommands, Runtime, World};

#[derive(Copy, Clone)]
struct Update;
impl runen_ecs::ScheduleLabel for Update {}

fn local_system(_: LocalCommands<'_>) {}

fn main() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, local_system);
}
''')

# Add direct World default/local proof and explicit local-system positive proof.
commands_test = Path("crates/runen-ecs/tests/commands.rs")
text = commands_test.read_text()
text = text.replace(
    "use std::panic::{AssertUnwindSafe, catch_unwind};\n",
    "use std::cell::Cell;\nuse std::panic::{AssertUnwindSafe, catch_unwind};\nuse std::rc::Rc;\n",
    1,
)
text += r'''

#[test]
fn world_entrypoints_make_transfer_safe_commands_the_default() {
    let mut world = World::new();
    world.insert_resource(Events(Vec::new()));

    let mut commands = world.commands();
    commands.queue(|world: &mut World| event(world, "default"));
    commands.apply(&mut world).unwrap();
    assert_eq!(world.resource::<Events>().unwrap().0, ["default"]);

    let local_ran = Rc::new(Cell::new(false));
    let captured = Rc::clone(&local_ran);
    let mut local_commands = world.local_commands();
    local_commands.queue(move |_world: &mut World| {
        captured.set(true);
        Ok(())
    });
    local_commands.apply(&mut world).unwrap();
    assert!(local_ran.get());
}

#[test]
fn local_commands_succeed_with_explicit_invoker_thread_registration() {
    let mut world = World::new();
    let local_ran = Rc::new(Cell::new(false));
    let captured = Rc::clone(&local_ran);
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (move |mut commands: LocalCommands<'_>| {
            let deferred_capture = Rc::clone(&captured);
            commands.queue(move |_world: &mut World| {
                deferred_capture.set(true);
                Ok(())
            });
        })
        .on_invoker_thread(),
    );
    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert!(local_ran.get());
}
'''
commands_test.write_text(text)

# Public surface: default recorder/batch in prelude, explicit local types root-only.
boundary = Path("crates/runen-ecs/tests/public_api_boundary.rs")
text = boundary.read_text()
needle = '    assert!(PRELUDE_RS.contains("Commands"));\n'
replacement = (
    '    assert!(PRELUDE_RS.contains("Commands"));\n'
    '    assert!(PRELUDE_RS.contains("BatchCommands"));\n'
    '    assert!(!PRELUDE_RS.contains("LocalCommands"));\n'
    '    assert!(!PRELUDE_RS.contains("LocalBatchCommands"));\n'
)
if needle not in text:
    raise SystemExit("prelude boundary assertion anchor not found")
boundary.write_text(text.replace(needle, replacement, 1))

# Document the normal/explicit-local authoring distinction.
readme = Path("crates/runen-ecs/README.md")
text = readme.read_text()
anchor = (
    "Ordinary system registration is the proven-transferable path. Use\n"
    "`.on_invoker_thread()` only when a system genuinely requires the thread that\n"
    "invokes its schedule. Transferable eligibility is not a promise that a system\n"
    "currently runs on a worker or runs in parallel.\n"
)
replacement = anchor + (
    "\n`Commands` is the normal deferred structural-mutation recorder and accepts only\n"
    "transfer-safe deferred work. Code that genuinely needs arbitrary local /\n"
    "`!Send` deferred work imports `LocalCommands` explicitly and pairs that system\n"
    "with `.on_invoker_thread()`. `BatchCommands` and `LocalBatchCommands` follow\n"
    "the same default-versus-explicit-local distinction.\n"
)
if anchor not in text:
    raise SystemExit("README mobility paragraph anchor not found")
readme.write_text(text.replace(anchor, replacement, 1))

# ADR 0002 remains the mobility decision; reconcile terminology without changing it.
adr2 = Path("docs/adr/0002-model-system-execution-mobility-as-a-proven-capability.md")
text = adr2.read_text()
decision_marker = "> **Decision date:** 2026-09-11\n"
note = (
    decision_marker
    + "\n> **Current command terminology (2026-09-14):** Later accepted deferred-command\n"
    + "> work added the transfer-safe recorder, and issue #69 normalized public names so\n"
    + "> `Commands` is that normal transfer-safe capability while `LocalCommands` is the\n"
    + "> explicit invoker-thread-only capability. This updates terminology only; the\n"
    + "> mobility proof and local-capability decision below are unchanged.\n"
)
if decision_marker not in text:
    raise SystemExit("ADR 0002 decision marker not found")
text = text.replace(decision_marker, note, 1)
text = text.replace(
    "| current ordinary LocalCommands | never transferable |",
    "| `Commands` | transferable when its transfer-safe deferred effect contract is satisfied |\n"
    "| `LocalCommands` | never transferable |",
)
section8 = re.compile(r"### 8\. .*?\n.*?(?=\n### 9\.)", re.S)
replacement8 = '''### 8. Deferred command capabilities preserve mobility explicitly

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
'''.rstrip()
text, count = section8.subn(replacement8, text, count=1)
if count != 1:
    raise SystemExit("ADR 0002 section 8 not found")
adr2.write_text(text)

# ADR 0003 keeps capability terminology but records the current public names.
adr3 = Path("docs/adr/0003-realize-deterministic-parallel-system-execution-from-serial-semantics.md")
text = adr3.read_text()
marker = re.search(r"> \*\*Decision date:\*\* [^\n]+\n", text)
if marker:
    note = (
        marker.group(0)
        + "\n> **Current command terminology (2026-09-14):** issue #69 names the\n"
        + "> transfer-safe default recorder `Commands` and the arbitrary local recorder\n"
        + "> `LocalCommands`; internal `TransferableDeferred` / transferable-buffer\n"
        + "> vocabulary remains capability terminology. Executor semantics are unchanged.\n"
    )
    text = text[:marker.start()] + note + text[marker.end():]
adr3.write_text(text)

# Renamed fixture paths are part of trybuild diagnostics.
for path in Path("crates/runen-ecs/tests/ui").glob("*.stderr"):
    text = path.read_text()
    text = text.replace("$DIR/transferable_commands_non_send.rs", "$DIR/commands_non_send.rs")
    text = text.replace("$DIR/transferable_spawn_non_send.rs", "$DIR/commands_spawn_non_send.rs")
    path.write_text(text)
