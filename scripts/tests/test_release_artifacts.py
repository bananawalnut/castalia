import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def load(name: str, relative: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


spdx = load("cargo_metadata_to_spdx", "scripts/cargo_metadata_to_spdx.py")
provenance = load("write_bootstrap_provenance", "scripts/write_bootstrap_provenance.py")
audit_ignores = load("audit_ignore_ids", "scripts/audit_ignore_ids.py")


class ReleaseArtifactTests(unittest.TestCase):
    def test_audit_ignore_parser_does_not_scrape_comments(self):
        source = '''
[advisories]
ignore = [
  # RUSTSEC-2000-0001 is documentation, not an exemption.
  "RUSTSEC-2000-0002",
]
'''
        self.assertEqual(audit_ignores.explicit_ignore_ids(source), ["RUSTSEC-2000-0002"])

    def test_spdx_contains_only_root_dependency_closure(self):
        metadata = {
            "packages": [
                {"id": "root 1", "name": "dregg-node", "version": "1", "license": "MIT", "source": None},
                {"id": "dep 1", "name": "serde", "version": "1", "license": "MIT OR Apache-2.0", "source": "registry+https://github.com/rust-lang/crates.io-index"},
                {"id": "other 1", "name": "unrelated", "version": "1", "license": None, "source": None},
            ],
            "resolve": {
                "nodes": [
                    {"id": "root 1", "dependencies": ["dep 1"]},
                    {"id": "dep 1", "dependencies": []},
                    {"id": "other 1", "dependencies": []},
                ]
            },
        }
        old_epoch = os.environ.get("SOURCE_DATE_EPOCH")
        os.environ["SOURCE_DATE_EPOCH"] = "0"
        try:
            result = spdx.build_sbom(metadata, "dregg-node")
        finally:
            if old_epoch is None:
                os.environ.pop("SOURCE_DATE_EPOCH", None)
            else:
                os.environ["SOURCE_DATE_EPOCH"] = old_epoch
        self.assertEqual(result["spdxVersion"], "SPDX-2.3")
        self.assertEqual({p["name"] for p in result["packages"]}, {"dregg-node", "serde"})
        self.assertEqual(result["creationInfo"]["created"], "1970-01-01T00:00:00Z")

    def test_provenance_rejects_marshal_only_status(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "dregg-node"
            sbom_file = Path(directory) / "sbom.json"
            binary.write_bytes(b"node")
            sbom_file.write_text("{}")
            with self.assertRaisesRegex(ValueError, "verified Lean"):
                provenance.build_provenance(
                    binary,
                    {"state_producer": "rust", "lean_producer": False, "federation_mode": "solo"},
                    sbom_file,
                )

    def test_provenance_accepts_the_arm64_release_pair(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "dregg-node"
            sbom_file = Path(directory) / "sbom.json"
            binary.write_bytes(b"node")
            sbom_file.write_text("{}")
            result = provenance.build_provenance(
                binary,
                {
                    "state_producer": "lean",
                    "lean_producer": True,
                    "federation_mode": "solo",
                },
                sbom_file,
                "castalia-bootstrap-node-linux-aarch64",
                "aarch64-unknown-linux-gnu",
            )
            self.assertEqual(result["artifact"], "castalia-bootstrap-node-linux-aarch64")
            self.assertEqual(result["build"]["target"], "aarch64-unknown-linux-gnu")

    def test_provenance_rejects_an_artifact_target_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "dregg-node"
            sbom_file = Path(directory) / "sbom.json"
            binary.write_bytes(b"node")
            sbom_file.write_text("{}")
            with self.assertRaisesRegex(ValueError, "requires target"):
                provenance.build_provenance(
                    binary,
                    {
                        "state_producer": "lean",
                        "lean_producer": True,
                        "federation_mode": "solo",
                    },
                    sbom_file,
                    "castalia-bootstrap-node-linux-aarch64",
                    "x86_64-unknown-linux-gnu",
                )


if __name__ == "__main__":
    unittest.main()
