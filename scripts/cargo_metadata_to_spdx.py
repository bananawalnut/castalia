#!/usr/bin/env python3
"""Create a deterministic SPDX 2.3 SBOM for one Cargo package and its closure."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path


SUPPORTED_TARGET_ARCHES = {
    "x86_64-unknown-linux-gnu": "x86_64",
    "aarch64-unknown-linux-gnu": "aarch64",
}


def spdx_id(package_id: str) -> str:
    digest = hashlib.sha256(package_id.encode("utf-8")).hexdigest()[:20]
    return f"SPDXRef-Package-{digest}"


def created_at() -> str:
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    value = dt.datetime.fromtimestamp(epoch, tz=dt.UTC)
    return value.strftime("%Y-%m-%dT%H:%M:%SZ")


def build_sbom(
    metadata: dict,
    root_name: str,
    target: str = "x86_64-unknown-linux-gnu",
) -> dict:
    if target not in SUPPORTED_TARGET_ARCHES:
        raise ValueError(f"unsupported bootstrap SBOM target: {target}")
    target_arch = SUPPORTED_TARGET_ARCHES[target]
    packages_by_id = {package["id"]: package for package in metadata["packages"]}
    resolve = metadata.get("resolve") or {}
    nodes = {node["id"]: node for node in resolve.get("nodes", [])}
    roots = [package for package in metadata["packages"] if package["name"] == root_name]
    if len(roots) != 1:
        raise ValueError(f"expected exactly one Cargo package named {root_name!r}, found {len(roots)}")
    root_id = roots[0]["id"]

    closure: set[str] = set()
    pending = [root_id]
    while pending:
        package_id = pending.pop()
        if package_id in closure:
            continue
        if package_id not in packages_by_id:
            raise ValueError(f"resolve graph references unknown package {package_id!r}")
        closure.add(package_id)
        pending.extend(nodes.get(package_id, {}).get("dependencies", []))

    packages = []
    for package_id in sorted(closure):
        package = packages_by_id[package_id]
        source = package.get("source")
        download = "NOASSERTION"
        if source and source.startswith("registry+"):
            download = f"https://crates.io/crates/{package['name']}/{package['version']}/download"
        packages.append(
            {
                "SPDXID": spdx_id(package_id),
                "name": package["name"],
                "versionInfo": package["version"],
                "downloadLocation": download,
                "filesAnalyzed": False,
                "licenseConcluded": "NOASSERTION",
                "licenseDeclared": package.get("license") or "NOASSERTION",
                "copyrightText": "NOASSERTION",
                "externalRefs": [
                    {
                        "referenceCategory": "PACKAGE-MANAGER",
                        "referenceType": "purl",
                        "referenceLocator": (
                            f"pkg:cargo/{package['name']}@{package['version']}"
                        ),
                    }
                ],
            }
        )

    relationships = [
        {
            "spdxElementId": "SPDXRef-DOCUMENT",
            "relationshipType": "DESCRIBES",
            "relatedSpdxElement": spdx_id(root_id),
        }
    ]
    for package_id in sorted(closure):
        for dependency_id in sorted(nodes.get(package_id, {}).get("dependencies", [])):
            if dependency_id in closure:
                relationships.append(
                    {
                        "spdxElementId": spdx_id(package_id),
                        "relationshipType": "DEPENDS_ON",
                        "relatedSpdxElement": spdx_id(dependency_id),
                    }
                )

    revision = os.environ.get("GITHUB_SHA", "unknown")
    namespace_seed = f"{root_name}:{revision}:{target}:{len(packages)}"
    namespace = hashlib.sha256(namespace_seed.encode("utf-8")).hexdigest()
    return {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"{root_name}-linux-{target_arch}",
        "documentNamespace": f"https://dregg.zenith-research.ca/spdx/{namespace}",
        "creationInfo": {
            "created": created_at(),
            "creators": ["Tool: scripts/cargo_metadata_to_spdx.py"],
        },
        "packages": packages,
        "relationships": relationships,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--metadata", required=True, type=Path)
    parser.add_argument("--root-package", required=True)
    parser.add_argument(
        "--target", required=True, choices=sorted(SUPPORTED_TARGET_ARCHES)
    )
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    metadata = json.loads(args.metadata.read_text())
    sbom = build_sbom(metadata, args.root_package, args.target)
    args.output.write_text(json.dumps(sbom, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
