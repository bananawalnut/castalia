import importlib.util
import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MEMBERSHIP_FILTERS = (
    ROOT / "deploy/aws-free-plan/verify-membership-cell.jq",
    ROOT / "deploy/oci/verify-membership-cell.jq",
)


def load(name: str, relative: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


spdx = load("cargo_metadata_to_spdx", "scripts/cargo_metadata_to_spdx.py")
provenance = load("write_bootstrap_provenance", "scripts/write_bootstrap_provenance.py")
audit_ignores = load("audit_ignore_ids", "scripts/audit_ignore_ids.py")


def field_hex(value: int) -> str:
    return f"{value:064x}"


class ReleaseArtifactTests(unittest.TestCase):
    def membership_cell(self):
        return {
            "found": True,
            "id": "11" * 32,
            "public_key": "22" * 32,
            "token_id": "33" * 32,
            "state_commitment": "44" * 32,
            "program_kind": "Cases",
            "program": {
                "kind": "Cases",
                "cases": [{
                    "guard": {"kind": "Always"},
                    "constraints": [
                        {"kind": "Immutable", "index": index}
                        for index in range(16)
                    ],
                }],
            },
            "fields": [
                field_hex(3_624_629_473_532_657_987),
                field_hex(2),
                field_hex(1),
                *[field_hex(0) for _ in range(9)],
                field_hex(1),
                field_hex(0),
                field_hex(0),
                field_hex(0),
            ],
            "capability_count": 0,
            "num_capabilities": 0,
            "has_delegate": False,
            "has_delegation": False,
            "delegate": None,
            "capabilities": [],
            "capability_tombstones": [],
        }

    def verify_membership_cell(self, cell, membership_filter=MEMBERSHIP_FILTERS[0]):
        return subprocess.run(
            [
                "jq", "-e",
                "--arg", "id", "11" * 32,
                "--arg", "owner", "22" * 32,
                "--arg", "token", "33" * 32,
                "--arg", "commitment", "44" * 32,
                "--arg", "magic", field_hex(3_624_629_473_532_657_987),
                "--arg", "zero", field_hex(0),
                "--arg", "one", field_hex(1),
                "--arg", "two", field_hex(2),
                "-f", str(membership_filter),
            ],
            input=json.dumps(cell),
            text=True,
            capture_output=True,
            check=False,
        )

    def test_deployment_soaks_accept_exact_snake_case_membership_cell(self):
        self.assertEqual(
            MEMBERSHIP_FILTERS[0].read_bytes(),
            MEMBERSHIP_FILTERS[1].read_bytes(),
            "AWS and OCI membership-cell verification drifted",
        )
        for membership_filter in MEMBERSHIP_FILTERS:
            with self.subTest(membership_filter=membership_filter):
                result = self.verify_membership_cell(
                    self.membership_cell(), membership_filter
                )
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_deployment_soak_constants_match_canonical_membership_vector(self):
        vector = json.loads(
            (ROOT / "docs/vectors/castalia-permissionless-membership-v2.vector.json")
            .read_text()
        )
        for soak in (
            ROOT / "deploy/aws-free-plan/soak-membership.sh",
            ROOT / "deploy/oci/soak-membership.sh",
        ):
            source = soak.read_text()
            for variable, key in (
                ("FACTORY_ID", "factoryId"),
                ("PROGRAM_ID", "programId"),
                ("TOKEN_ID", "tokenId"),
            ):
                with self.subTest(soak=soak, variable=variable):
                    match = re.search(
                        rf'^{variable}="([0-9a-f]{{64}})"$',
                        source,
                        re.MULTILINE,
                    )
                    self.assertIsNotNone(match, f"missing canonical {variable}")
                    self.assertEqual(match.group(1), vector[key])

    def test_deployment_soaks_reject_noncanonical_membership_cells(self):
        mutations = {}

        camel_case = self.membership_cell()
        camel_case["publicKey"] = camel_case.pop("public_key")
        mutations["camel-case wire projection"] = camel_case

        wrong_token = self.membership_cell()
        wrong_token["token_id"] = "55" * 32
        mutations["wrong token"] = wrong_token

        mutable = self.membership_cell()
        mutable["program"]["cases"][0]["constraints"][7]["kind"] = "Range"
        mutations["mutable field program"] = mutable

        changed_field = self.membership_cell()
        changed_field["fields"][12] = f"{2:064x}"
        mutations["non-active field"] = changed_field

        capability = self.membership_cell()
        capability["capability_count"] = 1
        mutations["capability"] = capability

        delegation = self.membership_cell()
        delegation["has_delegation"] = True
        mutations["delegation"] = delegation

        for membership_filter in MEMBERSHIP_FILTERS:
            for name, cell in mutations.items():
                with self.subTest(membership_filter=membership_filter, name=name):
                    result = self.verify_membership_cell(cell, membership_filter)
                    self.assertNotEqual(result.returncode, 0)

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
