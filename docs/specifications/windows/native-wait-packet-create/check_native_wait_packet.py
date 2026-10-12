#!/usr/bin/env python3
"""Validate native captures, artifact identities and nonempty validation gates."""
import argparse, hashlib, json, re, sys
from pathlib import Path
sys.dont_write_bytecode = True

def require(ok, message):
    if not ok:
        raise ValueError(message)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def rows(path):
    result = []
    for line in path.read_text().splitlines():
        if not line.startswith("case="):
            continue
        pairs = [part.split("=", 1) for part in line.split()]
        require(all(len(p) == 2 for p in pairs), "malformed capture field")
        require(len(pairs) == len({p[0] for p in pairs}), "duplicate capture field")
        result.append(dict(pairs))
    return result

def validate(base, source_root):
    require(not sys.flags.optimize, "optimized validation rejected")
    manifest = json.loads((base / "manifest.json").read_text())
    listed = {r["path"] for r in manifest["files"]}
    actual = {p.name for p in base.iterdir() if p.is_file() and p.name != "manifest.json"}
    require(len(listed) == len(manifest["files"]) and listed == actual, "manifest file set")
    for r in manifest["files"]:
        p = base / r["path"]
        require(p.stat().st_size == r["bytes"] and digest(p) == r["sha256"], "artifact identity: " + r["path"])
    total = excluded = 0
    for profile in ["arm64", "x64", "x86", "x86-laa"]:
        width = 4 if profile.startswith("x86") else 8
        for family, count in [("create", 317), ("lifetime", 18), ("parity", 7)]:
            stem = "native-wait-parity" if family == "parity" else "native-wait-packet-" + family
            path = base / f"{stem}-probe-{profile}.log"
            data = rows(path)
            require(len(data) == count and path.read_text().splitlines()[-1] == f"complete cases={count}", "native rows/terminal")
            for index, row in enumerate(data):
                n = lambda key, radix=16: int(row[key], radix)
                require(n("case", 10) == index and n("width", 10) == width, "ordinal/width")
                before, after = ("before", "after") if family == "parity" else ("handles-before", "handles-after")
                require(n(before, 10) == n(after, 10), "native handle leak")
                if family == "create":
                    require(n("exception") == 0, "external SEH")
                    for field in ["output", "output-backing", "attributes"]:
                        require(len(bytes.fromhex(row[field])) == n(field + "-captured", 10), "capture extent")
                    if row["label"] == "access":
                        require(n("status") == 0, "ordinary access constructor status")
                    if n("status") == 0:
                        access = n("access"); grant = access & 0x011F0001
                        if access & 0xE0000000: grant |= 0x20001
                        if access & 0x12000000: grant |= 0xF0001
                        require(n("grant") == grant, "packet generic mapping")
                        require(row["typename"] == "WaitCompletionPacket" and n("query-type") == 0 and n("query") == 0xC0000024 and n("close") == 0, "real distinct object query/close")
                        require((n("wait"), n("wait-error", 10)) == ((0, 0) if grant & 0x100000 else (0xFFFFFFFF, 5)), "public wait/access")
                    if n("mode", 10) in (11, 12, 13, 24, 25, 26) or (n("access") & 0x1000000 and n("status") == 0): excluded += 1
                elif family == "lifetime":
                    grant_sync = bool(n("access") & 0x100000)
                    expected = 0 if grant_sync else 0xC0000022
                    require(all(n(k) == expected for k in ["first", "second", "third", "alias-first", "alias-second", "after-multiple", "after-close"]), "notification state/lifetime")
                    require(n("status") == n("close-first") == n("close-last") == 0, "lifetime construction/close")
                    require((n("any"), n("any-error", 10)) == ((0, 0) if grant_sync else (0xFFFFFFFF, 5)), "alias wait-any")
                    require(n("all") == 0xFFFFFFFF and n("all-error", 10) == (87 if grant_sync else 5), "alias wait-all/access precedence")
                elif row["kind"] == "timer":
                    require(n("status") == 0 and n("first") == n("second") == 0x102 and n("error", 10) == n("second-error", 10) == 0 and n("close", 10) == 1, "dormant timer public waits")
                else:
                    require(n("all") == 0xFFFFFFFF and n("all-error", 10) == 87 and n("any") == 0 and n("single") == (0 if n("manual", 10) else 0x102), "event alias consumption")
            total += count
    require((total, excluded) == (1368, 40), "native observation/exclusion partition")
    identities = json.loads((base / "wait-packet-producer-identity.json").read_text())
    require(len(identities) == 12, "actual producer identities")
    for r in identities:
        stem = "native-wait-parity" if r["family"] == "parity" else "native-wait-packet-" + r["family"]
        require(r["source-sha256"] == digest(base / (stem + "-probe.cpp")), "actual producer source")
        machine = 0xAA64 if r["profile"] == "arm64" else 0x8664 if r["profile"] == "x64" else 0x14C
        require(int(r["machine"], 16) == machine and r["executable-bytes"] > 0, "actual producer PE")
    source_check = json.loads((base / "native-wait-packet-current-source-check.json").read_text())
    require(source_check["errors"] == [] and source_check["source-files"] == 16313, "verified native snapshot")
    archive = json.loads((base / "native-wait-packet-owning-archive-hashes.json").read_text())
    require(archive["core-features"] == [] and len(archive["sources"]) == 32, "owning source/core feature selection")
    require(len(archive["helpers"]) == len(archive["archives"]) == 2, "owning helper/archive roster")
    require(all(r["structural-member-walk"] == "pass" and r["bytes"] > 0 and r["members"] > 0 for r in archive["archives"]), "complete archive walks")
    if source_root:
        for r in archive["sources"]:
            p = source_root / r["path"]
            require(p.stat().st_size == r["bytes"] and digest(p) == r["sha256"], "current source/archive identity: " + r["path"])
    pair = json.loads((base / "ordinary-observer-pair.json").read_text())
    require(pair["before-sha256"] == pair["after-sha256"] == digest(base / "native-wait-packet-before-trace.rs") == digest(base / "native-wait-packet-after-trace.rs"), "identical ordinary observer")
    require(pair["before-turn"] == 42172 and pair["after-turn"] > pair["before-turn"], "ordinary frontier advance")
    require(pair["scheduler-call-delta"] == pair["after-turn"] - pair["before-turn"], "scheduler-call arithmetic")
    gates = json.loads((base / "gate-results.json").read_text())
    for record in gates:
        original = (base / record["original-path"]).read_text(encoding="utf-8-sig")
        require(record["test-results"] == re.findall(r"test result: [^\n]+", original), "gate summary/original log binding")
    for host in ["macos", "linux", "windows"]:
        for name in ["targeted", "threading", "timer2", "worker-factory", "events", "io-completion", "capi", "all-targets", "integration", "owning-shipping", "owning-cpp"]:
            matches = [r for r in gates if r["host"] == host and r["gate"] == name]
            require(len(matches) == 1 and matches[0]["exit-code"] == 0, "successful required gate: " + host + "/" + name)
            r = matches[0]
            if name in ["targeted", "timer2", "worker-factory", "events", "io-completion", "capi", "integration"]:
                expected = {"targeted": (7, 8), "timer2": (4, 5), "worker-factory": (5, 6), "events": (7, 13), "io-completion": (5, 6), "capi": (168, 168), "integration": (544, 4)}[name][host == "windows"]
                require(r["test-results"] and re.search(rf"ok\. {expected} passed; 0 failed; 0 ignored;", r["test-results"][0]), "nonempty selected count: " + host + "/" + name)
            if name == "threading":
                require(r["test-results"] and re.search(r"ok\. [1-9][0-9]* passed; 0 failed; 0 ignored;", r["test-results"][0]), "nonempty threading gate")
            if name == "all-targets":
                require("Finished `dev` profile" in (base / r["original-path"]).read_text(), "completed all-target build")
        full = [r for r in gates if r["host"] == host and r["gate"] == "full"]
        require(len(full) == 1 and full[0]["test-results"] and "0 filtered out" in full[0]["test-results"][0], "reported unfiltered full outcomes")
    require(any(r["host"] == "linux" and r["gate"] == "capi-parallel" and r["exit-code"] == 101 for r in gates), "retained original C API timeout")
    print("PASS: 1368 original observations; 1228 constructor replays/40 explicit exclusions; 32 source/archive identities")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-root", type=Path, default=Path(__file__).parent)
    parser.add_argument("--source-root", type=Path)
    args = parser.parse_args()
    validate(args.evidence_root, args.source_root)
