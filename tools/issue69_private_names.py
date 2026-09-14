from pathlib import Path


def replace_exact(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one occurrence of {old!r}, found {count}")
    p.write_text(text.replace(old, new, 1))


def replace_count(path: str, old: str, new: str, expected: int) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected} occurrences of {old!r}, found {count}")
    p.write_text(text.replace(old, new))


extract = "crates/runen-ecs/src/system/extract.rs"
replacements = [
    ("    commands: Option<NonNull<LocalCommands<'static>>>,\n    transferable_commands: Option<NonNull<Commands<'static>>>,",
     "    local_commands: Option<NonNull<LocalCommands<'static>>>,\n    commands: Option<NonNull<Commands<'static>>>,"),
    ("        commands: Option<&'world mut LocalCommands<'static>>,\n        transferable_commands: Option<&'world mut Commands<'static>>,",
     "        local_commands: Option<&'world mut LocalCommands<'static>>,\n        commands: Option<&'world mut Commands<'static>>,"),
    ("            commands: commands.map(NonNull::from),\n            transferable_commands: transferable_commands.map(NonNull::from),",
     "            local_commands: local_commands.map(NonNull::from),\n            commands: commands.map(NonNull::from),"),
    ("    pub(crate) fn commands(self) -> LocalCommands<'world> {",
     "    pub(crate) fn local_commands(self) -> LocalCommands<'world> {"),
    ("            self.commands\n                .expect(\"local command owner must be available for LocalCommands\")",
     "            self.local_commands\n                .expect(\"local command owner must be available for LocalCommands\")"),
    ("    pub(crate) fn transferable_commands(self) -> Commands<'world> {",
     "    pub(crate) fn commands(self) -> Commands<'world> {"),
    ("            self.transferable_commands\n                .expect(\"transferable command owner must be available for Commands\")",
     "            self.commands\n                .expect(\"command owner must be available for Commands\")"),
    ("        // Safety: the runtime constructs this pointer from the live transferable\n        // command owner and keeps it valid until extraction finishes. Only a",
     "        // Safety: the runtime constructs this pointer from the live transfer-safe\n        // command owner and keeps it valid until extraction finishes. Only a"),
    ("                .expect(\"transferable command owner must provide an external queue\")",
     "                .expect(\"command owner must provide an external queue\")"),
]
for old, new in replacements:
    replace_exact(extract, old, new)

params = "crates/runen-ecs/src/system/params.rs"
replace_exact(
    params,
    "        Ok(context.commands())\n    }\n}\n\nunsafe impl<'param> SystemParam for Commands<'param>",
    "        Ok(context.local_commands())\n    }\n}\n\nunsafe impl<'param> SystemParam for Commands<'param>",
)
replace_exact(params, "        Ok(context.transferable_commands())", "        Ok(context.commands())")

runtime = "crates/runen-ecs/src/system/runtime.rs"
# Rename the genuinely local owner first, including its local-branch references.
replace_exact(
    runtime,
    "                let mut commands = (deferred_recorder_class\n                    == DeferredRecorderClass::LocalDeferred)\n                    .then(LocalCommands::new_external_owner);",
    "                let mut local_commands = (deferred_recorder_class\n                    == DeferredRecorderClass::LocalDeferred)\n                    .then(LocalCommands::new_external_owner);",
)
replace_exact(
    runtime,
    "                        commands.as_mut(),\n                        transferable_commands.as_mut(),",
    "                        local_commands.as_mut(),\n                        transferable_commands.as_mut(),",
)
replace_exact(
    runtime,
    "                                commands\n                                    .expect(\"local command owner must exist for local recorder\")",
    "                                local_commands\n                                    .expect(\"local command owner must exist for local recorder\")",
)

# Both registration paths use the same transfer-safe owner; after the local owner
# is disambiguated, normalize that private variable to the ordinary public name.
replace_count(runtime, "transferable_commands", "commands", 6)
replace_count(
    runtime,
    "transferable command owner must exist for transferable recorder",
    "command owner must exist for transferable recorder",
    2,
)
