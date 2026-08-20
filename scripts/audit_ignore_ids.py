#!/usr/bin/env python3
"""Print only the explicitly quoted IDs in audit.toml's ignore array."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


def explicit_ignore_ids(text: str) -> list[str]:
    match = re.search(r"(?ms)^ignore\s*=\s*\[(.*?)^\]", text)
    if not match:
        raise ValueError("audit.toml has no [advisories].ignore array")
    ids = re.findall(r'^\s*"(RUSTSEC-[0-9]{4}-[0-9]{4})"\s*,?', match.group(1), re.MULTILINE)
    if len(ids) != len(set(ids)):
        raise ValueError("audit.toml contains a duplicate advisory ignore")
    return ids


if __name__ == "__main__":
    print("\n".join(explicit_ignore_ids((ROOT / "audit.toml").read_text())))
