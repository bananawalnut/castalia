import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "ci_nextest_shard", ROOT / "scripts" / "ci-nextest-shard.py"
)
assert SPEC is not None and SPEC.loader is not None
shards = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(shards)


class CiNextestShardTests(unittest.TestCase):
    def metadata(self):
        names = [
            "alpha",
            "beta",
            "deos-zed",
            "dregg-zkoracle-live",
            "gamma",
            "grain-verify-wasm",
            "servo-render",
            "starbridge-web",
        ]
        return {"packages": [{"name": name} for name in names]}

    def test_all_generic_packages_are_covered_once(self):
        names = shards.workspace_package_names(self.metadata())
        partitions = [shards.package_shard(names, index, 3) for index in range(3)]
        flattened = [name for partition in partitions for name in partition]

        self.assertEqual(sorted(flattened), names)
        self.assertEqual(len(flattened), len(set(flattened)))
        self.assertTrue(shards.SPLIT_TARGET_PACKAGES.isdisjoint(flattened))

    def test_sixteen_way_ci_partition_is_exact(self):
        names = [f"package-{index:03}" for index in range(227)]
        partitions = [shards.package_shard(names, index, 16) for index in range(16)]
        flattened = [name for partition in partitions for name in partition]

        self.assertEqual(sorted(flattened), names)
        self.assertEqual(len(flattened), len(set(flattened)))
        self.assertEqual(sorted(map(len, partitions)), [14] * 13 + [15] * 3)

    def test_config_only_names_tight_timeout_packages_in_scope(self):
        config = shards.render_nextest_config(["alpha", "dregg-zkoracle-live"])

        self.assertIn("default-filter = 'all()'", config)
        self.assertIn("package(dregg-zkoracle-live)", config)
        self.assertNotIn("package(servo-render)", config)

    def test_explicit_split_target_drift_fails_closed(self):
        metadata = self.metadata()
        metadata["packages"] = [
            package for package in metadata["packages"] if package["name"] != "starbridge-web"
        ]

        with self.assertRaisesRegex(ValueError, "split-target package set drifted"):
            shards.workspace_package_names(metadata)

    def test_invalid_or_empty_shards_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "0 <= index < count"):
            shards.package_shard(["alpha"], 1, 1)
        with self.assertRaisesRegex(ValueError, "is empty"):
            shards.package_shard(["alpha"], 1, 2)


if __name__ == "__main__":
    unittest.main()
