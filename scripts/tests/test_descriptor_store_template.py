#!/usr/bin/env python3

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = ROOT / "deploy" / "aws" / "descriptor-store.yml"
CONFIG = ROOT / ".config" / "descriptor-store.json"
HYDRATE_ACTION = ROOT / ".github" / "actions" / "hydrate-descriptors" / "action.yml"
PUBLISH_WORKFLOW = ROOT / ".github" / "workflows" / "publish-descriptors.yml"
BOOTSTRAP = ROOT / "scripts" / "bootstrap_descriptor_source.sh"

EXPECTED_FILENAMES = {
    "rotation-v3-setfield-value8-staged-registry.tsv",
    "rotation-v3-staged-registry.tsv",
    "rotation-wide-registry-staged.tsv",
    "rotation-wide-transfer-staged.tsv",
    "rotation-wide-umem-welded-registry-staged.tsv",
    "umem-cohort-multidomain-v1-staged-registry.tsv",
    "umem-cohort-v1-staged-registry.tsv",
}


class DescriptorStoreTemplateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = TEMPLATE.read_text()

    def test_bucket_is_private_versioned_encrypted_and_tls_only(self) -> None:
        for setting in (
            "BlockPublicAcls: true",
            "BlockPublicPolicy: true",
            "IgnorePublicAcls: true",
            "RestrictPublicBuckets: true",
            "ObjectOwnership: BucketOwnerEnforced",
            "SSEAlgorithm: AES256",
            "Status: Enabled",
            "aws:SecureTransport: \"false\"",
        ):
            self.assertIn(setting, self.text)

    def test_roles_cannot_list_delete_or_access_unrelated_objects(self) -> None:
        self.assertNotIn("s3:List", self.text)
        self.assertNotIn("s3:Delete", self.text)
        self.assertEqual(self.text.count("s3:PutObject"), 2)
        self.assertEqual(self.text.count("s3:GetObject"), 3)
        self.assertEqual(
            self.text.count(
                'Resource: !Sub "${DescriptorBucket.Arn}/descriptors/v1/sha256/*"'
            ),
            4,
        )

    def test_bucket_policy_requires_write_once_precondition(self) -> None:
        self.assertIn("DenyNonConditionalDescriptorWrites", self.text)
        self.assertIn('"s3:if-none-match": "true"', self.text)
        self.assertIn('"s3:ObjectCreationOperation": "true"', self.text)

    def test_deployment_role_is_ec2_only_and_read_only(self) -> None:
        self.assertIn("DescriptorDeploymentRole:", self.text)
        self.assertIn("Service: ec2.amazonaws.com", self.text)
        self.assertIn("DescriptorDeploymentInstanceProfile:", self.text)
        self.assertIn("DeploymentInstanceProfileName:", self.text)

    def test_publish_trust_is_environment_scoped(self) -> None:
        self.assertIn(
            "repo:bananawalnut/castalia:environment:descriptor-publish", self.text
        )
        self.assertIn("repo:bananawalnut/castalia:*", self.text)

    def test_repository_config_has_the_exact_allowlist(self) -> None:
        config = json.loads(CONFIG.read_text())
        self.assertEqual(config["schema_version"], 1)
        self.assertEqual(config["region"], "us-east-1")
        self.assertEqual(set(config["filenames"]), EXPECTED_FILENAMES)
        self.assertEqual(len(config["filenames"]), len(EXPECTED_FILENAMES))

    def test_new_remote_actions_and_installer_are_immutable(self) -> None:
        action_text = HYDRATE_ACTION.read_text()
        publish_text = PUBLISH_WORKFLOW.read_text()
        for text in (action_text, publish_text):
            remote_uses = re.findall(r"uses:\s+([^./\s][^@\s]+)@([^\s]+)", text)
            self.assertTrue(remote_uses)
            for action, revision in remote_uses:
                self.assertRegex(
                    revision,
                    r"^[0-9a-f]{40}$",
                    f"{action} is not pinned to a full commit SHA",
                )

        bootstrap = BOOTSTRAP.read_text()
        self.assertIn('ELAN_VERSION="v4.2.3"', bootstrap)
        for checksum in (
            "df0b2b3a439961ffcbb3985214365ffe40f49bc871df04dff268c7d8e21ca8b2",
            "cb69af0803b04157bc30201c29c12fca882bb3ad8b43476b8d2d3064810bc3ac",
            "10d037a69731c0593723e018130c5f54afde175796b4af8ba1317e561e55598c",
            "7cae4c03b2f0de4053fb04a91359d5804551e6e37a6ddd1b2e0097dc561ae4a9",
        ):
            self.assertIn(checksum, bootstrap)
        self.assertNotIn("/latest/", bootstrap)

    def test_deployment_hydrates_before_building(self) -> None:
        for relative in ("deploy/aws/setup.sh", "deploy/aws/update.sh"):
            script = (ROOT / relative).read_text()
            self.assertIn("install-descriptor-tools.sh", script)
            self.assertLess(
                script.index("scripts/descriptor_store.py fetch"),
                script.index("cargo build"),
            )


if __name__ == "__main__":
    unittest.main()
