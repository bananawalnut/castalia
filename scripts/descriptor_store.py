#!/usr/bin/env python3
"""Fetch, verify, and publish Castalia's content-addressed staged descriptors."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parent.parent
CONFIG_REL = Path(".config") / "descriptor-store.json"
PROVENANCE_REL = Path("circuit") / "descriptors" / "PROVENANCE.json"
DESCRIPTOR_REL = Path("circuit") / "descriptors"
LFS_HEADER = b"version https://git-lfs.github.com/spec/v1\n"


class DescriptorStoreError(RuntimeError):
    """A descriptor store operation failed closed."""


@dataclass(frozen=True)
class StoreConfig:
    region: str
    bucket: str
    prefix: str
    read_role_arn: str
    publish_role_arn: str
    filenames: tuple[str, ...]


@dataclass(frozen=True)
class Descriptor:
    filename: str
    sha256: str

    def key(self, prefix: str) -> str:
        return f"{prefix}/{self.sha256}/{self.filename}"


def _load_json(path: Path, label: str) -> dict:
    try:
        value = json.loads(path.read_text())
    except FileNotFoundError as exc:
        raise DescriptorStoreError(f"missing {label}: {path}") from exc
    except (OSError, json.JSONDecodeError) as exc:
        raise DescriptorStoreError(f"cannot read {label} {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise DescriptorStoreError(f"{label} must be a JSON object: {path}")
    return value


def _safe_filename(value: object) -> str:
    if not isinstance(value, str) or not value:
        raise DescriptorStoreError("descriptor filenames must be non-empty strings")
    parsed = PurePosixPath(value)
    if parsed.is_absolute() or parsed.name != value or value in {".", ".."}:
        raise DescriptorStoreError(f"unsafe descriptor filename: {value!r}")
    if "\\" in value or "\x00" in value:
        raise DescriptorStoreError(f"unsafe descriptor filename: {value!r}")
    return value


def load_store(root: Path = ROOT) -> tuple[StoreConfig, tuple[Descriptor, ...]]:
    config_path = root / CONFIG_REL
    raw = _load_json(config_path, "descriptor store configuration")
    if raw.get("schema_version") != 1:
        raise DescriptorStoreError(
            f"unsupported descriptor store schema_version in {config_path}"
        )

    required_strings = (
        "region",
        "bucket",
        "prefix",
        "read_role_arn",
        "publish_role_arn",
    )
    for field in required_strings:
        if not isinstance(raw.get(field), str) or not raw[field].strip():
            raise DescriptorStoreError(f"{config_path}: {field} must be a non-empty string")

    prefix = raw["prefix"].strip("/")
    if prefix != raw["prefix"] or not prefix:
        raise DescriptorStoreError(f"{config_path}: prefix must not start or end with '/'")
    if any(part in {"", ".", ".."} for part in PurePosixPath(prefix).parts):
        raise DescriptorStoreError(f"{config_path}: unsafe prefix {prefix!r}")

    filenames_raw = raw.get("filenames")
    if not isinstance(filenames_raw, list) or not filenames_raw:
        raise DescriptorStoreError(f"{config_path}: filenames must be a non-empty list")
    filenames = tuple(_safe_filename(value) for value in filenames_raw)
    if len(set(filenames)) != len(filenames):
        raise DescriptorStoreError(f"{config_path}: filenames contains duplicates")

    provenance = _load_json(root / PROVENANCE_REL, "descriptor provenance")
    hashes = provenance.get("descriptor_sha256")
    if not isinstance(hashes, dict):
        raise DescriptorStoreError("PROVENANCE.json has no descriptor_sha256 object")

    descriptors: list[Descriptor] = []
    for filename in filenames:
        digest = hashes.get(filename)
        if (
            not isinstance(digest, str)
            or len(digest) != 64
            or any(c not in "0123456789abcdef" for c in digest)
        ):
            raise DescriptorStoreError(
                f"PROVENANCE.json has no valid SHA-256 for {filename}"
            )
        descriptors.append(Descriptor(filename, digest))

    config = StoreConfig(
        region=raw["region"],
        bucket=raw["bucket"],
        prefix=prefix,
        read_role_arn=raw["read_role_arn"],
        publish_role_arn=raw["publish_role_arn"],
        filenames=filenames,
    )
    return config, tuple(descriptors)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _verify_one(path: Path, descriptor: Descriptor) -> None:
    if not path.is_file():
        raise DescriptorStoreError(f"missing descriptor: {path}")
    with path.open("rb") as handle:
        if handle.read(len(LFS_HEADER)) == LFS_HEADER:
            raise DescriptorStoreError(f"Git LFS pointer rejected: {path}")
    actual = sha256_file(path)
    if actual != descriptor.sha256:
        raise DescriptorStoreError(
            f"checksum mismatch for {descriptor.filename}: "
            f"expected {descriptor.sha256}, got {actual}"
        )


def verify_directory(
    directory: Path,
    descriptors: tuple[Descriptor, ...],
    *,
    source_only: bool,
) -> None:
    directory = directory.resolve()
    if not directory.is_dir():
        raise DescriptorStoreError(f"descriptor directory does not exist: {directory}")

    expected = {descriptor.filename for descriptor in descriptors}
    if source_only:
        actual = {entry.name for entry in directory.iterdir()}
    else:
        actual = {entry.name for entry in directory.glob("*.tsv")}
    unexpected = sorted(actual - expected)
    if unexpected:
        raise DescriptorStoreError(
            "unexpected descriptor files: " + ", ".join(unexpected)
        )

    for descriptor in descriptors:
        _verify_one(directory / descriptor.filename, descriptor)


def _aws_cli() -> str:
    return os.environ.get("DESCRIPTOR_STORE_AWS_CLI", "aws")


def _run_aws(args: list[str], *, allow_failure: bool = False) -> subprocess.CompletedProcess:
    try:
        result = subprocess.run(
            [_aws_cli(), *args],
            check=False,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError as exc:
        raise DescriptorStoreError(f"AWS CLI not found: {_aws_cli()}") from exc
    if result.returncode != 0 and not allow_failure:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostic"
        raise DescriptorStoreError(f"AWS CLI failed ({result.returncode}): {detail}")
    return result


def _download(config: StoreConfig, descriptor: Descriptor, destination: Path) -> None:
    uri = f"s3://{config.bucket}/{descriptor.key(config.prefix)}"
    _run_aws(
        [
            "s3",
            "cp",
            uri,
            str(destination),
            "--region",
            config.region,
            "--only-show-errors",
        ]
    )


def _atomic_install(
    staged: Path,
    destination: Path,
    descriptors: tuple[Descriptor, ...],
) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="descriptor-backup-", dir=destination.parent) as raw:
        backup = Path(raw)
        installed: list[Path] = []
        moved: list[tuple[Path, Path]] = []
        try:
            for descriptor in descriptors:
                target = destination / descriptor.filename
                if target.exists():
                    saved = backup / descriptor.filename
                    os.replace(target, saved)
                    moved.append((saved, target))
            for descriptor in descriptors:
                target = destination / descriptor.filename
                os.replace(staged / descriptor.filename, target)
                installed.append(target)
        except Exception:
            for target in installed:
                try:
                    target.unlink()
                except FileNotFoundError:
                    pass
            for saved, target in reversed(moved):
                os.replace(saved, target)
            raise


def fetch(root: Path = ROOT) -> None:
    config, descriptors = load_store(root)
    destination = root / DESCRIPTOR_REL
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix="descriptor-fetch-", dir=destination.parent
    ) as raw:
        staged = Path(raw)
        for descriptor in descriptors:
            _download(config, descriptor, staged / descriptor.filename)
        verify_directory(staged, descriptors, source_only=True)
        _atomic_install(staged, destination, descriptors)
    verify_directory(destination, descriptors, source_only=False)
    print(f"descriptor_store: fetched and verified {len(descriptors)} descriptors")


def install(source_dir: Path, root: Path = ROOT) -> None:
    """Atomically install an already-produced, provenance-matching descriptor set."""
    _config, descriptors = load_store(root)
    source_dir = source_dir.resolve()
    verify_directory(source_dir, descriptors, source_only=True)

    destination = root / DESCRIPTOR_REL
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix="descriptor-install-", dir=destination.parent
    ) as raw:
        staged = Path(raw)
        for descriptor in descriptors:
            shutil.copyfile(
                source_dir / descriptor.filename,
                staged / descriptor.filename,
            )
        verify_directory(staged, descriptors, source_only=True)
        _atomic_install(staged, destination, descriptors)
    verify_directory(destination, descriptors, source_only=False)
    print(f"descriptor_store: installed and verified {len(descriptors)} descriptors")


def verify(root: Path = ROOT, source_dir: Path | None = None) -> None:
    _config, descriptors = load_store(root)
    directory = source_dir if source_dir is not None else root / DESCRIPTOR_REL
    verify_directory(directory, descriptors, source_only=source_dir is not None)
    print(f"descriptor_store: verified {len(descriptors)} descriptors in {directory}")


def _conditional_put(
    config: StoreConfig,
    descriptor: Descriptor,
    source: Path,
) -> None:
    """Create an immutable key, treating a failed write-once precondition as success."""
    result = _run_aws(
        [
            "s3api",
            "put-object",
            "--bucket",
            config.bucket,
            "--key",
            descriptor.key(config.prefix),
            "--body",
            str(source),
            "--if-none-match",
            "*",
            "--region",
            config.region,
        ],
        allow_failure=True,
    )
    if result.returncode == 0:
        return
    detail = f"{result.stdout}\n{result.stderr}".lower()
    if (
        "preconditionfailed" in detail
        or "precondition failed" in detail
        or "412" in detail
    ):
        return
    raise DescriptorStoreError(
        "conditional immutable S3 upload failed: "
        + (result.stderr.strip() or result.stdout.strip() or str(result.returncode))
    )


def publish(source_dir: Path, root: Path = ROOT) -> None:
    config, descriptors = load_store(root)
    source_dir = source_dir.resolve()
    verify_directory(source_dir, descriptors, source_only=True)

    with tempfile.TemporaryDirectory(prefix="descriptor-publish-verify-") as raw:
        downloaded = Path(raw)
        for descriptor in descriptors:
            _conditional_put(
                config,
                descriptor,
                source_dir / descriptor.filename,
            )
            _download(config, descriptor, downloaded / descriptor.filename)
        verify_directory(downloaded, descriptors, source_only=True)
    print(f"descriptor_store: published and reverified {len(descriptors)} descriptors")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("fetch", help="download and atomically install all descriptors")
    install_parser = subparsers.add_parser(
        "install", help="atomically install a verified local descriptor set"
    )
    install_parser.add_argument("--source-dir", type=Path, required=True)
    verify_parser = subparsers.add_parser("verify", help="verify the hydrated descriptors")
    verify_parser.add_argument("--source-dir", type=Path)
    publish_parser = subparsers.add_parser("publish", help="publish immutable descriptors")
    publish_parser.add_argument("--source-dir", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        if args.command == "fetch":
            fetch()
        elif args.command == "install":
            install(args.source_dir)
        elif args.command == "verify":
            verify(source_dir=args.source_dir)
        elif args.command == "publish":
            publish(args.source_dir)
    except DescriptorStoreError as exc:
        print(f"descriptor_store: FAIL — {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
