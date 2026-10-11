#!/usr/bin/env python3
"""Validate original constructor captures; reject optimized/partial evidence."""
import hashlib
import argparse
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True


def require(condition, message):
    if not condition:
        raise ValueError(message)


def parse(path):
    return [
        dict(part.split("=", 1) for part in line.split())
        for line in path.read_text().splitlines()
        if line.startswith("case=")
    ]


def validate(folder, source_root=None):
    require(not sys.flags.optimize, "optimized validation is rejected")
    manifest = json.loads((folder / "manifest.json").read_text())
    entries = manifest["files"]
    names = [entry["path"] for entry in entries]
    require(len(names) == len(set(names)), "duplicate manifest path")
    actual = {
        str(path.relative_to(folder))
        for path in folder.rglob("*")
        if path.is_file() and path.name != "manifest.json"
    }
    require(actual == set(names), "manifest must cover the exact evidence set")
    for entry in entries:
        data = (folder / entry["path"]).read_bytes()
        require(len(data) == entry["bytes"], "size mismatch: " + entry["path"])
        require(hashlib.sha256(data).hexdigest() == entry["sha256"], "hash mismatch: " + entry["path"])
    require(manifest["original-observations"] == 1296, "original observation count")
    require(manifest["replayed-observations"] == 1184, "replay observation count")
    require(manifest["explicit-policy-exclusions"] == 112, "policy exclusion count")
    total = 0
    excluded = 0
    for arch in ("arm64", "x64", "x86", "x86-laa"):
        width = 4 if arch.startswith("x86") else 8
        for kind, count in (("create", 303), ("root", 21)):
            name = f"native-io-completion-{kind}-probe-{arch}.log"
            rows = parse(folder / name)
            require(len(rows) == count, "capture count: " + name)
            require([int(row["case"]) for row in rows] == list(range(count)), "capture order: " + name)
            for row in rows:
                require(int(row["width"]) == width, "capture pointer width")
                require(row["exception"] == "00000000", "external SEH exception")
                require(row["handles-before"] == row["handles-after"], "native handle leak")
                status = int(row["status"], 16)
                access = int(row["access"], 16)
                mode = int(row["mode"])
                if mode in (11, 12, 13, 24, 25, 26) or (access & 0x01000000 and status == 0):
                    excluded += 1
                if kind == "root":
                    flags = int(row["flags"], 16)
                    expected = 0xC000000D if flags & ~0x1EE2 else 0xC0000033 if int(row["offset"]) else 0
                    require(status == expected, "RootDirectory/flag precedence")
                    if status:
                        require(int(row["handle"], 16) == (0 if width == 4 else 0xA5A5A5A5A5A5A5A5), "failure output publication")
                if row["label"] == "concurrency":
                    require(status == 0, "unsigned concurrency acceptance")
                if row["label"] == "attributes-flags":
                    flags = int(row["flags"], 16)
                    require(status == (0xC000000D if flags & ~0x1EE2 else 0), "port flag acceptance")
                for prefix in ("output", "output-backing", "attributes"):
                    require(len(row[prefix]) == 2 * int(row[prefix + "-captured"]), "captured extent")
                    bytes.fromhex(row[prefix])
                if status == 0:
                    handle = int(row["handle"], 16)
                    require(handle != 0 and handle % 4 == 0, "successful handle identity")
                    require(int(row["handle-bytes"]) == width, "successful handle extent")
                    require(row["close"] == "00000000", "native close status")
            total += len(rows)
    require(total == 1296 and excluded == 112, "complete capture partition")
    identities = json.loads((folder / "io-completion-producer-identities.json").read_text())
    require([row["profile"] for row in identities] == ["arm64", "x64", "x86", "x86-laa"], "producer profiles")
    for row in identities:
        arch = row["profile"]
        require(int(row["pe-machine"], 16) == (0xAA64 if arch == "arm64" else 0x8664 if arch == "x64" else 0x14C), "producer PE machine")
        if arch.startswith("x86"):
            require(row["large-address-aware"] == (arch == "x86-laa"), "producer LAA identity")
        for item in row["files"]:
            name = item["path"].replace("\\", "/").rsplit("/", 1)[-1]
            if name.endswith(".exe"):
                require(item["bytes"] > 0 and len(item["sha256"]) == 64, "recorded native executable identity")
                continue
            data = (folder / name).read_bytes()
            require(len(data) == item["bytes"] and hashlib.sha256(data).hexdigest() == item["sha256"], "producer source/raw identity")
    if source_root is not None:
        sources = json.loads((folder / "owning-source-hashes.json").read_text())
        require(len(sources) == 18, "owning source coverage")
        for item in sources:
            data = (source_root / item["path"]).read_bytes()
            require(len(data) == item["bytes"] and hashlib.sha256(data).hexdigest() == item["sha256"], "current owning source identity: " + item["path"])
    print("PASS: 1296 original observations, 1184 modeled rows, 112 explicit policy exclusions")


if __name__ == "__main__":
    try:
        parser = argparse.ArgumentParser()
        parser.add_argument("folder", nargs="?", type=Path, default=Path(__file__).parent)
        parser.add_argument("--source-root", type=Path)
        args = parser.parse_args()
        validate(args.folder, args.source_root)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print("FAIL:", error, file=sys.stderr)
        raise SystemExit(1)
