#!/usr/bin/env python3
"""Reject version drift before building an installer (Python 3.10+)."""
import json
import re
from pathlib import Path

root = Path(__file__).resolve().parent.parent
versions = [json.loads((root / p).read_text())["version"] for p in
            ("package.json", "package-lock.json", "src-tauri/tauri.conf.json")]
versions.append(json.loads((root / "package-lock.json").read_text())["packages"][""]["version"])
for path in ("src-tauri/Cargo.toml", "src-tauri/Cargo.lock"):
    text = (root / path).read_text()
    match = re.search(r'name = "zapbeam"\s+version = "([^"]+)"', text)
    assert match, f"Missing application version in {path}"
    versions.append(match[1])
assert set(versions) == {"0.1.0"}, f"Version drift: {versions}"
print("All application versions: 0.1.0")
