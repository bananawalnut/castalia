#!/usr/bin/env python3
"""Regression tests for the production audit lockfile boundary."""

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("production_audit_lock.py")
spec = importlib.util.spec_from_file_location("production_audit_lock", SCRIPT)
if spec is None or spec.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
production_audit_lock = importlib.util.module_from_spec(spec)
spec.loader.exec_module(production_audit_lock)


class ProductionAuditReachabilityTests(unittest.TestCase):
    def test_ci_audits_generated_production_lock_without_quick_xml_ignore(self) -> None:
        workflow = SCRIPT.parent.parent.joinpath(".github/workflows/ci.yml").read_text()

        self.assertIn("scripts/production_audit_lock.py", workflow)
        self.assertIn("--file /tmp/castalia-production.lock", workflow)
        self.assertNotIn('"RUSTSEC-2026-0194"', workflow)
        self.assertNotIn('"RUSTSEC-2026-0195"', workflow)

    def test_lock_parser_keeps_audit_identity_fields(self) -> None:
        lockfile = production_audit_lock.parse_lockfile(
            '''version = 4

[[package]]
name = "quick-xml"
version = "0.39.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc123"
dependencies = ["serde"]
'''
        )

        self.assertEqual(
            lockfile["package"],
            [
                {
                    "name": "quick-xml",
                    "version": "0.39.4",
                    "source": "registry+https://github.com/rust-lang/crates.io-index",
                    "checksum": "abc123",
                }
            ],
        )

    def test_reachable_packages_exclude_desktop_and_dev_only_dependencies(self) -> None:
        metadata = {
            "packages": [
                {"id": "cli", "name": "dregg-cli"},
                {"id": "node", "name": "dregg-node"},
                {"id": "core", "name": "dregg-core"},
                {"id": "desktop", "name": "starbridge-v2"},
                {"id": "quick", "name": "quick-xml"},
                {"id": "test", "name": "test-helper"},
            ],
            "resolve": {
                "nodes": [
                    {"id": "cli", "deps": [{"pkg": "core", "dep_kinds": [{"kind": None}]}]},
                    {
                        "id": "node",
                        "deps": [
                            {"pkg": "core", "dep_kinds": [{"kind": None}]},
                            {"pkg": "test", "dep_kinds": [{"kind": "dev"}]},
                        ],
                    },
                    {"id": "core", "deps": []},
                    {"id": "desktop", "deps": [{"pkg": "quick", "dep_kinds": [{"kind": None}]}]},
                    {"id": "quick", "deps": []},
                    {"id": "test", "deps": []},
                ]
            },
        }

        reachable = production_audit_lock.reachable_package_ids(
            metadata, {"dregg-cli", "dregg-node"}
        )

        self.assertEqual(reachable, {"cli", "node", "core"})

    def test_unknown_production_package_fails_closed(self) -> None:
        metadata = {
            "packages": [{"id": "cli", "name": "dregg-cli"}],
            "resolve": {"nodes": [{"id": "cli", "deps": []}]},
        }

        with self.assertRaisesRegex(ValueError, "missing production package"):
            production_audit_lock.reachable_package_ids(
                metadata, {"dregg-cli", "dregg-node"}
            )


if __name__ == "__main__":
    unittest.main()
