"""Relocate only an existing TokenPulse-owned root notify command and its restore metadata."""
import json
from pathlib import Path
import re
import sys
import tomllib

old_directory, new_directory, registry_directory = map(Path, sys.argv[1:])
old_executable = str(old_directory / "token-pulse-desktop.exe")
new_executable = str(new_directory / "token-pulse-desktop.exe")
updated = 0
for file in registry_directory.glob("*.registration.json"):
    record = json.loads(file.read_bytes())
    arguments = record["restore"]["installed_arguments"]
    if arguments[0].casefold() != old_executable.casefold():
        continue
    if len(arguments) != 4 or arguments[1:3] != ["--tokenpulse-notify", "--integration"]:
        raise RuntimeError("Invalid owned notification registration")
    config = Path(record["codex_home"]) / "config.toml"
    next_arguments = [new_executable, *arguments[1:]]
    if config.is_file():
        original = config.read_bytes()
        text = original.decode("utf-8-sig")
        if tomllib.loads(text).get("notify") == arguments:
            # The root table precedes the first table header. Parsing above verifies ownership;
            # replacing only the array span preserves comments, newline style and all other keys.
            first_table = re.search(r"(?m)^\s*\[", text)
            root_end = first_table.start() if first_table else len(text)
            matches = list(re.finditer(r"(?m)^[ \t]*notify[ \t]*=[ \t]*(\[[^\]]*\])", text[:root_end]))
            if len(matches) != 1:
                raise RuntimeError("Unsupported owned notify syntax; config was not changed")
            start, end = matches[0].span(1)
            edited = text[:start] + json.dumps(next_arguments, ensure_ascii=False) + text[end:]
            if tomllib.loads(edited).get("notify") != next_arguments:
                raise RuntimeError("Relocated notification failed validation")
            if config.read_bytes() != original:
                raise RuntimeError("Codex configuration changed during relocation")
            prefix = b"\xef\xbb\xbf" if original.startswith(b"\xef\xbb\xbf") else b""
            config.write_bytes(prefix + edited.encode("utf-8"))
    record["restore"]["installed_arguments"] = next_arguments
    # In-place writes retain the current-user protected ACL applied by the migration.
    file.write_bytes(json.dumps(record, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
    updated += 1
print(f"OWNED_NOTIFY_PATHS_RELOCATED: {updated}")
