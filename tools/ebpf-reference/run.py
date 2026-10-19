#!/usr/bin/env python3
"""Verify/extract the pinned primary source archive and run its differential tests."""

import hashlib
import os
from pathlib import Path
import subprocess
import tarfile


REVISION = "3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b"
SHA256 = "51b3da377e76a9ae3ee1852a089d81ccf904b686cb90a517f03a1766e0eec127"


def extract(root):
    """Verify the pinned archive and extract it to target/ebpf-reference-source.

    Shared with tools/ebpf-formal/run.py, whose SBPF proofs compile the same
    upstream source.
    """
    archive = root / "docs/specifications/ebpf/sbpf/upstream-3e1510b1.tar.gz"
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise SystemExit("Pinned upstream source archive checksum mismatch")
    destination = root / "target/ebpf-reference-source"
    prefix = f"sbpf-{REVISION}/"
    expected_files = set()
    with tarfile.open(archive) as source:
        for member in source.getmembers():
            if member.isdir() and member.name == prefix.rstrip("/"):
                continue
            if not member.name.startswith(prefix):
                raise SystemExit(f"Unexpected archive prefix: {member.name}")
            relative = Path(member.name[len(prefix):])
            if relative.is_absolute() or ".." in relative.parts:
                raise SystemExit(f"Invalid archive member: {member.name}")
            if member.isdir():
                continue
            if not member.isfile():
                raise SystemExit(f"Unsupported archive member: {member.name}")
            expected_files.add(relative)
            content = source.extractfile(member).read()
            path = destination / relative
            if path.exists() and path.read_bytes() != content:
                raise SystemExit(f"Modified upstream cache file: {path}")
            path.parent.mkdir(parents=True, exist_ok=True)
            if not path.exists():
                path.write_bytes(content)
    for path in destination.rglob("*"):
        if path.is_file() and path.relative_to(destination) not in expected_files:
            raise SystemExit(f"Unexpected upstream cache file: {path}")


def main():
    root = Path(__file__).resolve().parents[2]
    extract(root)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(root / "target/ebpf-reference")
    # Release mode: the exhaustive campaigns execute millions of programs.
    # Overflow-checked coverage of the same streams is the engine ebpf_diff
    # corpus replay. RAX_SBPF_CORPUS_OUT=1 rewrites the checked-in corpus.
    command = ["cargo", "+stable", "test", "--release", "--locked", "--manifest-path",
               str(root / "tools/ebpf-reference/Cargo.toml"), "--", "--nocapture"]
    raise SystemExit(subprocess.run(command, cwd=root, env=environment).returncode)


if __name__ == "__main__":
    main()
