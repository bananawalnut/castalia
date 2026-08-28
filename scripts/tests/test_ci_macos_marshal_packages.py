from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "ci_macos_marshal_packages", ROOT / "scripts" / "ci-macos-marshal-packages.py"
)
assert SPEC is not None and SPEC.loader is not None
marshal = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(marshal)


class MacosMarshalPackagesTests(unittest.TestCase):
    @staticmethod
    def metadata(kind: str = "dev") -> dict:
        return {
            "packages": [
                {"name": "alpha", "dependencies": []},
                {
                    "name": "bravo",
                    "dependencies": [{"name": marshal.TESTKIT, "kind": kind}],
                },
                {"name": marshal.TESTKIT, "dependencies": []},
            ]
        }

    def test_selects_only_packages_without_the_testkit(self) -> None:
        selected, excluded = marshal.package_sets(
            self.metadata(), {"bravo", marshal.TESTKIT}
        )
        self.assertEqual(selected, ["alpha"])
        self.assertEqual(excluded, ["bravo", marshal.TESTKIT])

    def test_manifest_drift_fails_closed(self) -> None:
        with self.assertRaisesRegex(ValueError, "metadata-only: bravo"):
            marshal.package_sets(self.metadata(), {marshal.TESTKIT})

    def test_non_dev_testkit_edge_requires_classifier_review(self) -> None:
        with self.assertRaisesRegex(ValueError, "normal dependency"):
            marshal.package_sets(
                self.metadata("normal"), {"bravo", marshal.TESTKIT}
            )

    def test_generated_config_keeps_full_profile_and_resolvable_override(self) -> None:
        config = marshal.render_nextest_config(["alpha", "servo-render"])
        self.assertIn("default-filter = 'all()'", config)
        self.assertIn("package(servo-render)", config)
        self.assertNotIn("dregg-zkoracle-live", config)


if __name__ == "__main__":
    unittest.main()
