#!/usr/bin/env python3
"""Verify that a source archive contains the seven provenance-pinned TSVs."""

from __future__ import annotations

import argparse
import hashlib
import json
import tarfile
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parent.parent
PROVENANCE = ROOT / "circuit" / "descriptors" / "PROVENANCE.json"
STORE_CONFIG = ROOT / ".config" / "descriptor-store.json"


def verify_archive(archive: Path) -> None:
    provenance_hashes = json.loads(PROVENANCE.read_text())["descriptor_sha256"]
    filenames = json.loads(STORE_CONFIG.read_text())["filenames"]
    hashes = {filename: provenance_hashes[filename] for filename in filenames}
    with tarfile.open(archive, "r:*") as source:
        regular_files = {
            PurePosixPath(member.name): member
            for member in source.getmembers()
            if member.isfile()
        }
        for filename, expected in hashes.items():
            suffix = ("circuit", "descriptors", filename)
            matches = [
                member
                for path, member in regular_files.items()
                if path.parts[-3:] == suffix
            ]
            if len(matches) != 1:
                raise SystemExit(
                    f"{archive}: expected one {filename}, found {len(matches)}"
                )
            handle = source.extractfile(matches[0])
            if handle is None:
                raise SystemExit(f"{archive}: cannot read {filename}")
            actual = hashlib.sha256(handle.read()).hexdigest()
            if actual != expected:
                raise SystemExit(
                    f"{archive}: checksum mismatch for {filename}: "
                    f"expected {expected}, got {actual}"
                )
    print(f"verified {len(hashes)} descriptor compiler inputs in {archive}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="+", type=Path)
    args = parser.parse_args()
    for archive in args.archives:
        verify_archive(archive)


if __name__ == "__main__":
    main()
