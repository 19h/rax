#!/usr/bin/env python3
"""Validate retained native constructor evidence, independently of RAX replay."""
import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rows(path):
    result = []
    for line in path.read_text().splitlines():
        if not line.startswith("case="):
            continue
        fields = [item.split("=", 1) for item in line.split()]
        require(all(len(item) == 2 for item in fields), "invalid row field")
        require(len(fields) == len({item[0] for item in fields}), "duplicate row field")
        result.append(dict(fields))
    return result


def validate(base, source_root):
    require(not sys.flags.optimize, "optimized validation is rejected")
    manifest = json.loads((base / "manifest.json").read_text())
    files = manifest["files"]
    actual = {p.name for p in base.iterdir() if p.is_file() and p.name != "manifest.json"}
    require(len(files) == len({r["path"] for r in files}), "duplicate manifest path")
    require(actual == {r["path"] for r in files}, "incomplete manifest file set")
    for item in files:
        path = base / item["path"]
        require(path.stat().st_size == item["bytes"] and digest(path) == item["sha256"],
                "file identity: " + item["path"])
    total = modeled = excluded = 0
    for family, count in [("create", 410), ("basic", 439), ("order", 56),
                          ("default-stack", 1)]:
        profiles = ["arm64", "x86"] if family == "default-stack" else ["arm64", "x64", "x86", "x86-laa"]
        for profile in profiles:
            path = base / f"native-worker-factory-{family}-probe-{profile}.log"
            data = rows(path)
            require(len(data) == count, f"{family}/{profile}: row count")
            require(path.read_text().splitlines()[-1] == f"complete cases={count}",
                    f"{family}/{profile}: terminal marker")
            width = 4 if profile.startswith("x86") else 8
            for index, row in enumerate(data):
                n = lambda key, radix=16: int(row[key], radix)
                require(n("case", 10) == index and n("width", 10) == width, "ordinal/width")
                require(n("exception") == 0, "external SEH")
                require(n("handles-before", 10) == n("handles-after", 10), "native handle leak")
                for field in ("output", "output-backing", "attributes"):
                    raw = bytes.fromhex(row[field])
                    require(len(raw) == n(field + "-captured", 10), "capture extent")
                require(n("output-guard", 10) in (0, 256) and n("attrs-guard", 10) in (0, 256), "guard state")
                status = n("status")
                if status == 0:
                    require(n("handle") != 0 and n("handle") % 4 == 0, "real native handle")
                    require(n("handle-bytes", 10) == width and n("close") == 0, "publication/close")
                    access = n("access")
                    grant = access & 0x010F00FF
                    for generic, specific in [(0x80000000, 0x20008), (0x40000000, 0x20004),
                                              (0x20000000, 0x20003), (0x10000000, 0xF00FF),
                                              (0x02000000, 0xF00FF)]:
                        if access & generic:
                            grant |= specific
                    require(n("grant") == grant, "worker access mapping")
                    if n("query") == 0:
                        worker = bytes.fromhex(row["worker"])
                        require(len(worker) == 128, "worker capture size")
                        require(n("returned") == (120 if width == 8 else 96), "basic native extent")
                        word = lambda at, size=4: int.from_bytes(worker[at:at + size], "little")
                        require(word(40) == n("count"), "raw maximum")
                        require(all(word(at) == 0 for at in (36, 44, 48, 52, 56)), "dormant worker counts")
                        stack = 96 if width == 8 else 84
                        require(word(stack, width) == (n("reserve") or 65536), "stack reserve/default")
                        require(word(stack + width, width) == (n("commit") or 4096), "stack commit/default")
                if row["label"] == "root-flags":
                    expected = 0xC000000D if n("flags") & ~0x1EF2 else 0xC0000033 if n("offset", 10) else 0
                    require(status == expected, "root/flag precedence")
                if row["label"] == "access-handle-order":
                    expected = 0xC0000022 if n("port-access") & 2 == 0 else 0xC0000008 if n("process-kind", 10) in (2, 3) else 0xC0000024
                    require(status == expected, "port access/process handle precedence")
                if row["label"] == "flags-handle-order":
                    require(status == (0xC0000008 if n("port-kind", 10) == 1 else 0xC0000024), "handles before flags")
                if family in ("basic", "order"):
                    if n("mode", 10) in (11, 12, 13, 24, 25, 26) or (n("access") & 0x01000000 and status == 0):
                        excluded += 1
                    else:
                        modeled += 1
            if family == "order":
                require("worker-type status=00000000 name=TpWorkerFactory" in path.read_text(), "native type name")
                life = [dict(t.split("=", 1) for t in line.split()[1:])
                        for line in path.read_text().splitlines() if line.startswith("lifetime ")]
                require([r["phase"] for r in life] == ["before", "created", "duplicated", "first-closed", "final-closed"], "lifetime phases")
                handles = [int(r["process-handles"]) for r in life]
                require(handles == [handles[0], handles[0]+1, handles[0]+1, handles[0]+1, handles[0]], "native internal process handle lifetime")
                # Query pointer counts contain fast-reference bias and change
                # on ordinary queries; do not assert exact kernel refcounts.
            total += count
    require((total, modeled, excluded) == (3622, 1868, 112), "original/replay partition")
    identities = json.loads((base / "worker-factory-default-stack-identity.json").read_text())
    require(len(identities) == 6, "corrected PE identities")
    for item in identities:
        expected = (2097152, 32768) if item["family"] == "default-stack" else (1048576, 4096)
        require((item["PE-stack-reserve"], item["PE-stack-commit"]) == expected,
                "actual PE stack header")
        require(item["source-sha256"] == digest(base / f"native-worker-factory-{item['family']}-probe.cpp"),
                "native producer source identity")
    archive = json.loads((base / "native-worker-factory-owning-archive-hashes.json").read_text())
    require(archive["core-features"] == [] and len(archive["sources"]) == 22, "archive/source feature scope")
    require(len(archive["helpers"]) == 2 and len(archive["archives"]) == 2, "owning identities")
    require(all(a["structural-member-walk"] == "pass" and a["members"] > 0 and a["bytes"] > 0 for a in archive["archives"]), "actual archive walk")
    require(archive["sources"] == json.loads((base / "owning-source-hashes.json").read_text()), "source/archive binding")
    if source_root:
        for item in archive["sources"]:
            path = source_root / item["path"]
            require(path.stat().st_size == item["bytes"] and digest(path) == item["sha256"], "current source: " + item["path"])
    pair = json.loads((base / "ordinary-observer-pair.json").read_text())
    require(pair["before-sha256"] == pair["after-sha256"], "identical observer")
    require(pair["before-sha256"] == digest(base / "native-worker-factory-before-trace.rs")
            and pair["after-sha256"] == digest(base / "native-worker-factory-after-trace.rs"),
            "observer source identity")
    require(pair["before-turn"] == 42076 and pair["after-turn"] == 42165 and pair["scheduler-call-delta"] == 89, "ordinary delta")
    print("PASS: 3622 original observations; 1868 replays/112 explicit exclusions; 22 source/archive identities")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-root", type=Path, default=Path(__file__).parent)
    parser.add_argument("--source-root", type=Path)
    args = parser.parse_args()
    validate(args.evidence_root, args.source_root)
