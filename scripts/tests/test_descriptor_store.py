#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import descriptor_store as ds  # noqa: E402


FAKE_AWS = r'''#!/usr/bin/env python3
import json
import os
import shutil
import sys
from pathlib import Path

store = Path(os.environ["FAKE_AWS_STORE"])
log = Path(os.environ["FAKE_AWS_LOG"])
args = sys.argv[1:]
with log.open("a") as handle:
    handle.write(json.dumps(args) + "\n")

def object_path(uri):
    rest = uri.removeprefix("s3://")
    _bucket, key = rest.split("/", 1)
    return store / key

if args[:2] == ["s3api", "head-object"]:
    key = args[args.index("--key") + 1]
    target = store / key
    if target.is_file():
        print(json.dumps({"ContentLength": target.stat().st_size}))
        raise SystemExit(0)
    print("An error occurred (404) when calling HeadObject: Not Found", file=sys.stderr)
    raise SystemExit(255)

if args[:2] == ["s3", "cp"]:
    source, destination = args[2], args[3]
    if source.startswith("s3://"):
        source_path = object_path(source)
        if os.environ.get("FAKE_AWS_FAIL_DOWNLOAD") == Path(destination).name:
            print("injected download failure", file=sys.stderr)
            raise SystemExit(2)
        if not source_path.is_file():
            print("An error occurred (404): Not Found", file=sys.stderr)
            raise SystemExit(1)
        destination_path = Path(destination)
        destination_path.parent.mkdir(parents=True, exist_ok=True)
        if os.environ.get("FAKE_AWS_CORRUPT_DOWNLOAD") == destination_path.name:
            destination_path.write_bytes(b"corrupt")
        else:
            shutil.copyfile(source_path, destination_path)
    else:
        destination_path = object_path(destination)
        destination_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination_path)
    raise SystemExit(0)

print("unsupported fake AWS invocation", args, file=sys.stderr)
raise SystemExit(3)
'''


class DescriptorStoreTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "repo"
        self.store = Path(self.temp.name) / "s3"
        self.log = Path(self.temp.name) / "aws.log"
        self.fake_aws = Path(self.temp.name) / "aws"
        self.fake_aws.write_text(FAKE_AWS)
        self.fake_aws.chmod(self.fake_aws.stat().st_mode | stat.S_IXUSR)
        self.store.mkdir()
        (self.root / ".config").mkdir(parents=True)
        (self.root / "circuit" / "descriptors").mkdir(parents=True)
        self.payloads = {
            "alpha-staged.tsv": b"alpha\n",
            "beta-staged.tsv": b"beta\n",
        }
        self._write_metadata(list(self.payloads))
        self.env = mock.patch.dict(
            os.environ,
            {
                "DESCRIPTOR_STORE_AWS_CLI": str(self.fake_aws),
                "FAKE_AWS_STORE": str(self.store),
                "FAKE_AWS_LOG": str(self.log),
            },
            clear=False,
        )
        self.env.start()

    def tearDown(self) -> None:
        self.env.stop()
        self.temp.cleanup()

    def _write_metadata(self, filenames: list[str]) -> None:
        config = {
            "schema_version": 1,
            "region": "us-east-1",
            "bucket": "test-bucket",
            "prefix": "descriptors/v1/sha256",
            "read_role_arn": "arn:aws:iam::123456789012:role/read",
            "publish_role_arn": "arn:aws:iam::123456789012:role/publish",
            "filenames": filenames,
        }
        (self.root / ds.CONFIG_REL).write_text(json.dumps(config))
        provenance = {
            "descriptor_sha256": {
                name: hashlib.sha256(self.payloads.get(name, b"")).hexdigest()
                for name in filenames
            }
        }
        (self.root / ds.PROVENANCE_REL).write_text(json.dumps(provenance))

    def _seed_store(self) -> None:
        config, descriptors = ds.load_store(self.root)
        for descriptor in descriptors:
            target = self.store / descriptor.key(config.prefix)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(self.payloads[descriptor.filename])

    def test_successful_hydration(self) -> None:
        self._seed_store()
        ds.fetch(self.root)
        ds.verify(self.root)
        destination = self.root / ds.DESCRIPTOR_REL
        for name, payload in self.payloads.items():
            self.assertEqual((destination / name).read_bytes(), payload)

    def test_checksum_mismatch_preserves_existing_files(self) -> None:
        self._seed_store()
        destination = self.root / ds.DESCRIPTOR_REL
        originals = {name: b"old-" + payload for name, payload in self.payloads.items()}
        for name, payload in originals.items():
            (destination / name).write_bytes(payload)
        with mock.patch.dict(os.environ, {"FAKE_AWS_CORRUPT_DOWNLOAD": "beta-staged.tsv"}):
            with self.assertRaisesRegex(ds.DescriptorStoreError, "checksum mismatch"):
                ds.fetch(self.root)
        for name, payload in originals.items():
            self.assertEqual((destination / name).read_bytes(), payload)

    def test_partial_download_rolls_back_before_install(self) -> None:
        self._seed_store()
        destination = self.root / ds.DESCRIPTOR_REL
        originals = {name: b"existing-" + payload for name, payload in self.payloads.items()}
        for name, payload in originals.items():
            (destination / name).write_bytes(payload)
        with mock.patch.dict(os.environ, {"FAKE_AWS_FAIL_DOWNLOAD": "beta-staged.tsv"}):
            with self.assertRaisesRegex(ds.DescriptorStoreError, "AWS CLI failed"):
                ds.fetch(self.root)
        for name, payload in originals.items():
            self.assertEqual((destination / name).read_bytes(), payload)

    def test_lfs_pointer_is_rejected_even_when_hash_pinned(self) -> None:
        pointer = (
            b"version https://git-lfs.github.com/spec/v1\n"
            b"oid sha256:0123456789abcdef\nsize 1\n"
        )
        self.payloads = {"alpha-staged.tsv": pointer}
        self._write_metadata(["alpha-staged.tsv"])
        (self.root / ds.DESCRIPTOR_REL / "alpha-staged.tsv").write_bytes(pointer)
        with self.assertRaisesRegex(ds.DescriptorStoreError, "Git LFS pointer rejected"):
            ds.verify(self.root)

    def test_path_traversal_in_config_is_rejected(self) -> None:
        self._write_metadata(["../escape-staged.tsv"])
        with self.assertRaisesRegex(ds.DescriptorStoreError, "unsafe descriptor filename"):
            ds.load_store(self.root)

    def test_missing_configuration_is_rejected(self) -> None:
        (self.root / ds.CONFIG_REL).unlink()
        with self.assertRaisesRegex(ds.DescriptorStoreError, "missing descriptor store"):
            ds.load_store(self.root)

    def test_publication_is_idempotent_and_reverified(self) -> None:
        source = Path(self.temp.name) / "source"
        source.mkdir()
        for name, payload in self.payloads.items():
            (source / name).write_bytes(payload)

        ds.publish(source, self.root)
        ds.publish(source, self.root)

        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        uploads = [
            call
            for call in calls
            if call[:2] == ["s3", "cp"] and not call[2].startswith("s3://")
        ]
        self.assertEqual(len(uploads), len(self.payloads))
        config, descriptors = ds.load_store(self.root)
        for descriptor in descriptors:
            self.assertEqual(
                (self.store / descriptor.key(config.prefix)).read_bytes(),
                self.payloads[descriptor.filename],
            )


if __name__ == "__main__":
    unittest.main()
