#!/usr/bin/env python3
"""Write fail-closed provenance for the verified Castalia bootstrap binary."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout.strip()


def pinned_mathlib_revision() -> str:
    lakefile = (ROOT / "metatheory" / "lakefile.toml").read_text()
    match = re.search(r'^rev = "([0-9a-f]{40})"$', lakefile, re.MULTILINE)
    if not match:
        raise ValueError("metatheory/lakefile.toml has no 40-hex mathlib revision")
    return match.group(1)


def build_provenance(binary: Path, status: dict, sbom: Path) -> dict:
    if status.get("state_producer") != "lean" or status.get("lean_producer") is not True:
        raise ValueError("runtime smoke did not report the verified Lean state producer")
    if status.get("federation_mode") != "solo":
        raise ValueError("runtime smoke did not report solo federation mode")

    revision = os.environ.get("GITHUB_SHA") or git("rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("release revision must be a full 40-hex Git commit")
    binary_sha = sha256(binary)
    sbom_sha = sha256(sbom)
    descriptor_provenance = ROOT / "circuit" / "descriptors" / "PROVENANCE.json"

    return {
        "schemaVersion": 1,
        "artifact": "castalia-bootstrap-node-linux-x86_64",
        "revision": revision,
        "stateProducer": "lean",
        "leanProducer": True,
        "federationMode": "solo",
        "binary": {"name": "dregg-node", "sha256": binary_sha},
        "sbom": {"name": "dregg-node.spdx.json", "sha256": sbom_sha},
        "build": {
            "requireLean": True,
            "target": "x86_64-unknown-linux-gnu",
            "rustToolchain": (ROOT / "rust-toolchain.toml").read_text().strip(),
            "leanToolchain": (ROOT / "metatheory" / "lean-toolchain").read_text().strip(),
            "mathlibRevision": pinned_mathlib_revision(),
            "descriptorProvenanceSha256": sha256(descriptor_provenance),
            "repository": os.environ.get("GITHUB_SERVER_URL", "https://github.com")
            + "/"
            + os.environ.get("GITHUB_REPOSITORY", "bananawalnut/castalia"),
            "workflowRun": os.environ.get("GITHUB_RUN_ID", "local"),
        },
        "runtimeAssertion": {
            "state_producer": status["state_producer"],
            "lean_producer": status["lean_producer"],
            "federation_mode": status["federation_mode"],
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--status", required=True, type=Path)
    parser.add_argument("--sbom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    provenance = build_provenance(
        args.binary, json.loads(args.status.read_text()), args.sbom
    )
    args.output.write_text(json.dumps(provenance, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
