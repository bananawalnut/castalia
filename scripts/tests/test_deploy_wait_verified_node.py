from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WAITS = (
    ROOT / "deploy" / "aws-free-plan" / "wait-for-verified-node.sh",
    ROOT / "deploy" / "oci" / "wait-for-verified-node.sh",
)


VERIFIED_STATUS = {
    "federation_mode": "solo",
    "state_producer": "lean",
    "lean_producer": True,
    "healthy": True,
    "consensus_live": True,
}


class WaitForVerifiedNodeTests(unittest.TestCase):
    def run_wait(
        self,
        status: dict | None,
        *,
        wait: Path = WAITS[0],
        curl_exit: int = 0,
        service_active_exit: int = 0,
        timeout: int = 1,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as raw:
            fake_bin = Path(raw)
            (fake_bin / "curl").write_text(
                "#!/bin/sh\n"
                "printf '%s\\n' \"$FAKE_STATUS\"\n"
                "exit \"$FAKE_CURL_EXIT\"\n"
            )
            (fake_bin / "systemctl").write_text(
                "#!/bin/sh\n"
                "if [ \"${1:-}\" = is-active ]; then\n"
                "  exit \"$FAKE_SERVICE_ACTIVE_EXIT\"\n"
                "fi\n"
                "exit 0\n"
            )
            (fake_bin / "journalctl").write_text("#!/bin/sh\nexit 0\n")
            (fake_bin / "sleep").write_text("#!/bin/sh\nexit 0\n")
            for path in fake_bin.iterdir():
                path.chmod(0o755)

            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{fake_bin}:{env['PATH']}",
                    "FAKE_STATUS": json.dumps(status or {}),
                    "FAKE_CURL_EXIT": str(curl_exit),
                    "FAKE_SERVICE_ACTIVE_EXIT": str(service_active_exit),
                }
            )
            return subprocess.run(
                [str(wait), "http://127.0.0.1:8420/status", str(timeout)],
                cwd=ROOT,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )

    def test_accepts_only_complete_verified_status(self) -> None:
        for wait in WAITS:
            with self.subTest(wait=wait):
                result = self.run_wait(VERIFIED_STATUS, wait=wait)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), VERIFIED_STATUS)

    def test_rejects_marshal_only_status(self) -> None:
        status = VERIFIED_STATUS | {
            "state_producer": "marshal-only",
            "lean_producer": False,
        }
        for wait in WAITS:
            with self.subTest(wait=wait):
                result = self.run_wait(status, wait=wait)
                self.assertEqual(result.returncode, 1)
                self.assertIn("did not become ready within 1 seconds", result.stderr)

    def test_fails_immediately_when_service_exits(self) -> None:
        for wait in WAITS:
            with self.subTest(wait=wait):
                result = self.run_wait(
                    None,
                    wait=wait,
                    curl_exit=22,
                    service_active_exit=3,
                    timeout=900,
                )
                self.assertEqual(result.returncode, 1)
                self.assertIn("service exited before readiness", result.stderr)

    def test_every_lifecycle_caller_uses_the_15_minute_gate(self) -> None:
        for relative in (
            "deploy/aws-free-plan/install.sh",
            "deploy/aws-free-plan/rollback-binary.sh",
            "deploy/aws-free-plan/soak-membership.sh",
            "deploy/aws-free-plan/create-encrypted-backup.sh",
            "deploy/aws-free-plan/restore-backup.sh",
            "deploy/oci/install.sh",
            "deploy/oci/rollback-binary.sh",
            "deploy/oci/soak-membership.sh",
            "deploy/oci/create-encrypted-backup.sh",
            "deploy/oci/restore-backup.sh",
        ):
            source = (ROOT / relative).read_text()
            self.assertIn("wait-for-verified-node.sh", source, relative)
            self.assertIn("900", source, relative)


if __name__ == "__main__":
    unittest.main()
