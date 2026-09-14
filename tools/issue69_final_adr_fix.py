from pathlib import Path

p = Path("docs/adr/0003-realize-deterministic-parallel-system-execution-from-serial-semantics.md")
text = p.read_text()
old = "There is no safe conversion between ordinary and transferable deferred effects/buffers,"
new = "There is no safe conversion between local and transferable deferred effects/buffers,"
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected exactly one stale ordinary/transferable conversion sentence, found {count}")
p.write_text(text.replace(old, new, 1))
