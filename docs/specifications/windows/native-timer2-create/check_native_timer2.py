#!/usr/bin/env python3
"""Validate original Timer2 evidence without depending on emulator output."""
import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
sys.dont_write_bytecode = True

def require(condition, message):
    if not condition:
        raise ValueError(message)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def rows(path, prefix="case="):
    result = []
    for line in path.read_text().splitlines():
        if not line.startswith(prefix):
            continue
        parts = line.split() if prefix == "case=" else line.split()[1:]
        fields = [part.split("=", 1) for part in parts]
        require(all(len(part) == 2 for part in fields), "malformed field")
        require(len(fields) == len({part[0] for part in fields}), "duplicate field")
        result.append(dict(fields))
    return result

def validate(base, source_root):
    require(not sys.flags.optimize, "optimized validation is rejected")
    manifest = json.loads((base / "manifest.json").read_text())
    items = manifest["files"]
    listed = {item["path"] for item in items}
    actual = {p.name for p in base.iterdir() if p.is_file() and p.name != "manifest.json"}
    require(len(listed) == len(items) and listed == actual, "incomplete manifest file set")
    for item in items:
        path = base / item["path"]
        require(path.stat().st_size == item["bytes"] and digest(path) == item["sha256"], "file identity: " + item["path"])
    total = excluded = 0
    allowed = {1, 5, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19}
    denied = {2, 3, 4, 6, 7, 17}
    for family, count in [("create", 835), ("order", 842), ("reduced", 267), ("alias", 154)]:
        for profile in ["arm64", "x64", "x86", "x86-laa"]:
            path = base / f"native-timer2-{family}-probe-{profile}.log"
            data = rows(path)
            require(len(data) == count, "native row count")
            require(path.read_text().splitlines()[-1] == f"complete cases={count}", "terminal marker")
            width = 4 if profile.startswith("x86") else 8
            for index, row in enumerate(data):
                n = lambda key, radix=16: int(row[key], radix)
                status, attributes, access = n("status"), n("count"), n("access")
                require(n("case", 10) == index and n("width", 10) == width, "ordinal/width")
                require(n("exception") == 0, "external SEH")
                require(n("handles-before", 10) == n("handles-after", 10), "native handle leak")
                for name in ["output", "output-backing", "attributes", "id", "id-backing"]:
                    require(len(bytes.fromhex(row[name])) == n(name + "-captured", 10), "byte capture extent")
                mode = n("mode", 10)
                wow_copy_fault = width == 4 and status in (0xC0000005, 0x80000001) and (
                    mode in (3, 4, 16, 17, 23) or
                    mode == 29 and n("id-mode", 10) in (3, 4))
                if attributes & ~0x8000000E or attributes & 10 == 10:
                    require(status == 0xC00000F2 or wow_copy_fault, "Attributes validation and WoW failure copy")
                elif mode >= 5 and mode not in (23, 28, 29):
                    require(status == 0xC00000F1 or wow_copy_fault, "non-null OA rejected before capture")
                if row["label"] == "reduced-id-range":
                    value = n("id-value")
                    expected = 0 if value in allowed else 0xC0000022 if value in denied else 0xC000000D
                    require(status == expected, "reduced UserMode ID role outcome")
                if status == 0:
                    require(n("query-object") == 0 and n("query") == 0xC0000024 and n("close") == 0, "real distinct IRTimer query/close")
                    require(n("handle-flags", 10) == 0, "noninherited handle")
                    grant = access & 0x011F0003
                    for generic, specific in [(0x80000000, 0x20001), (0x40000000, 0x20002), (0x20000000, 0x120000), (0x10000000, 0x1F0003), (0x02000000, 0x1F0003)]:
                        if access & generic:
                            grant |= specific
                    require(n("grant") == grant, "native timer access mapping")
                if access & 0x01000000 and status in (0, 0xC0000061):
                    excluded += 1
            if family == "reduced":
                require("privileges-enabled=0" in path.read_text().splitlines()[0], "reduced token")
                types = rows(path, "type ")
                require(len(types) == 6, "type/wait probes")
                require(all(r["name"] == "IRTimer" and int(r["wait"], 16) == 0x102 and int(r["query"], 16) == 0 for r in types), "native type and dormant waits")
            total += count
    require((total, total - excluded, excluded) == (8392, 8288, 104), "observation partition")
    identities = json.loads((base / "timer2-producer-identity.json").read_text())
    require(len(identities) == 16, "actual producer identities")
    for item in identities:
        require(item["source-sha256"] == digest(base / f"native-timer2-{item['family']}-probe.cpp"), "actual producer source")
        expected = 0xAA64 if item["profile"] == "arm64" else 0x8664 if item["profile"] == "x64" else 0x14C
        require(int(item["machine"], 16) == expected and item["executable-bytes"] > 0, "actual producer PE machine")
    archive = json.loads((base / "native-timer2-owning-archive-hashes.json").read_text())
    require(archive["core-features"] == [] and len(archive["sources"]) == 27, "empty-feature owning source scope")
    require(len(archive["helpers"]) == 2 and len(archive["archives"]) == 2, "owning helper/archive identities")
    require(all(a["structural-member-walk"] == "pass" and a["members"] > 0 and a["bytes"] > 0 for a in archive["archives"]), "actual archive member walk")
    require(archive["sources"] == json.loads((base / "owning-source-hashes.json").read_text()), "source/archive binding")
    if source_root:
        for item in archive["sources"]:
            path = source_root / item["path"]
            require(path.stat().st_size == item["bytes"] and digest(path) == item["sha256"], "current owning source: " + item["path"])
    pair = json.loads((base / "ordinary-observer-pair.json").read_text())
    require(pair["before-sha256"] == pair["after-sha256"], "identical observer")
    require(pair["before-sha256"] == digest(base / "native-timer2-before-trace.rs") and pair["after-sha256"] == digest(base / "native-timer2-after-trace.rs"), "actual observer bytes")
    require(pair["before-turn"] == 42165 and pair["after-turn"] > pair["before-turn"], "ordinary advance")
    require(pair["scheduler-call-delta"] == pair["after-turn"] - pair["before-turn"], "scheduler-call calculation")
    gates = json.loads((base / "gate-results.json").read_text())
    for host in ["macos", "linux", "windows"]:
        for name in ["targeted", "worker-factory", "events", "io-completion", "capi", "all-targets", "integration", "owning-shipping", "owning-cpp"]:
            selected = [g for g in gates if g["host"] == host and g["gate"] == name]
            require(len(selected) == 1 and selected[0]["exit-code"] == 0, "successful required gate: " + host + "/" + name)
            if name in ("targeted", "worker-factory", "events", "io-completion", "capi", "integration"):
                expected = {"targeted": (4, 5), "worker-factory": (5, 6),
                            "events": (7, 12), "io-completion": (5, 6),
                            "capi": (168, 168), "integration": (544, 4)}[name][host == "windows"]
                results = selected[0]["test-results"]
                require(results and re.search(rf"ok\. {expected} passed; 0 failed; 0 ignored;", results[0]), "nonempty selected test count: " + host + "/" + name)
            if name == "all-targets":
                require("Finished `dev` profile" in (base / selected[0]["original-path"]).read_text(), "completed all-target build")
        full = [g for g in gates if g["host"] == host and g["gate"] == "full"]
        require(len(full) == 1 and full[0]["test-results"], "reported full-suite outcomes")
    print("PASS: 8392 original observations; 8288 exact replays/104 explicit exclusions; 27 source/archive identities")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-root", type=Path, default=Path(__file__).parent)
    parser.add_argument("--source-root", type=Path)
    args = parser.parse_args()
    validate(args.evidence_root, args.source_root)
