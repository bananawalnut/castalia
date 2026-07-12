#!/usr/bin/env python3
"""Contract checks for WirePresentation claim and CI execution boundaries."""

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SEAM_DOC = ROOT / "docs/CREDENTIALS-WIRE-PRESENTATION-SEAM.md"
FIXTURE_DOC = ROOT / "credentials/fixtures/wire_presentation/README.md"


class WirePresentationDocumentationContractTests(unittest.TestCase):
    def test_docs_deny_membership_and_authority_source_claims(self) -> None:
        for path in (SEAM_DOC, FIXTURE_DOC):
            with self.subTest(path=path.relative_to(ROOT)):
                text = " ".join(path.read_text().lower().split())
                self.assertIn("neither provisions nor recognizes membership", text)
                self.assertIn("does not obtain", text)
                self.assertIn("live signed authority source", text)
                self.assertIn("externally injected", text)
                for unsupported_claim in (
                    "membership recognition",
                    "live authority",
                    "federation status",
                    "finality",
                    "provenance",
                ):
                    self.assertIn(unsupported_claim, text)


if __name__ == "__main__":
    unittest.main()
