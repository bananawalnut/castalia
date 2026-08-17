#!/usr/bin/env python3

from __future__ import annotations

import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = ROOT / "deploy" / "aws" / "descriptor-store.yml"
CONFIG = ROOT / ".config" / "descriptor-store.json"

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
        self.assertEqual(self.text.count("s3:PutObject"), 1)
        self.assertEqual(self.text.count("s3:GetObject"), 2)
        self.assertEqual(
            self.text.count(
                'Resource: !Sub "${DescriptorBucket.Arn}/descriptors/v1/sha256/*"'
            ),
            2,
        )

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


if __name__ == "__main__":
    unittest.main()
