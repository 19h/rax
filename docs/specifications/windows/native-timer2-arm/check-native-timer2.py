"""Check independent native captures and identity/coverage, without RAX imports."""
from pathlib import Path
import collections
import hashlib
import json
import sys

D = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent
PROFILES = {"arm64": (8, 0xAA64, True), "x64": (8, 0x8664, True),
            "x86": (4, 0x14C, False), "x86-laa": (4, 0x14C, True)}
SOURCES = {
    "native-v1": ("native-timer2-arm-v1", 842, "4182abf0bdbc8e0b59ba7b331b734c78e1d167dba52f68f335bde4e7367fb33f"),
    "native-fault-v2": ("native-timer2-arm-fault-v2", 3806, "5ff0b72b55f19f3204b3c9f7c9e5c30b2b9e6ee6d0af40e2814fbf16bab1c67d"),
    "native-state-v3": ("native-timer2-arm-state-v3", 13, "3d5e161da6d4a3dbdb8929c0799215e4b3ded4eac20f1e3704cadf70e9afc7bb"),
    "native-period-v4": ("native-timer2-arm-period-v4", 24, "15edfa4272b01393d213495543ca7db00dbb08dcd7cd0bcbf7134652281239a5"),
}


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode("utf-16" if raw[:2] in [b"\xff\xfe", b"\xfe\xff"] else "utf-8-sig"))


def number(row, key, radix=16):
    return int(row[key], radix)


def typed(kind, access):
    if kind in [4, 5]:
        return 0xC0000008
    if kind != 0:
        return 0xC0000024
    if access & (2 | 0x02000000 | 0x10000000 | 0x40000000) == 0:
        return 0xC0000022
    return 0


def value_status(row):
    if number(row, "due-present", 10) == 0:
        return 0xC00000F0
    failure = typed(number(row, "kind", 10), number(row, "access"))
    if failure:
        return failure
    tolerance = number(row, "tolerance")
    tolerance = tolerance - (1 << 64) if tolerance & (1 << 63) else tolerance
    if number(row, "params-present", 10) and (number(row, "version") != 0 or tolerance < -1):
        return 0xC00000F1
    if number(row, "attributes") & 2 and number(row, "period-present", 10) and number(row, "period") != 0:
        return 0xC000000D
    return 0


def fault_status(row, wow):
    guards = {a: number(row, a + "-guard-before", 10) for a in ["due", "period", "params"]}
    order = ["params", "null", "due", "period"] if wow else ["null", "due", "period", "params"]
    for stage in order:
        if stage == "null":
            if number(row, "due-mode", 10) == 1:
                return 0xC00000F0, guards
            continue
        mode = number(row, stage + "-mode", 10)
        if mode == 1:
            continue
        if mode == 4 and guards[stage]:
            guards[stage] = 0
            return 0x80000001, guards
        if mode in [2, 3, 9, 10, 11, 16] or stage == "params" and mode in [12, 13]:
            return 0xC0000005, guards
    code = typed(number(row, "kind", 10), number(row, "access"))
    if code == 0 and number(row, "params-mode", 10) != 1 and number(row, "version") != 0:
        code = 0xC00000F1
    return code, guards


report = {"profiles": {}, "captured-cases": 0, "replayed-records": 0,
          "scope": "Four ABI profiles, one Windows ARM64 kernel; producer hashes before/after, exact status/guard formulas and bounded state/timing evidence. Poll timing races are not exact goldens."}
for sub, (prefix, expected, source_hash) in SOURCES.items():
    base = D / sub
    assert hashlib.sha256((base / (prefix + ".cpp")).read_bytes()).hexdigest() == source_hash, prefix
    outcomes = load(base / (prefix + "-outcomes.json"))
    assert len(outcomes) == 4 and {r["profile"] for r in outcomes} == set(PROFILES)
    assert all(r["exit-code"] == 0 for r in outcomes)
    modules = load(base / (prefix + "-modules-before.json"))
    assert modules == load(base / (prefix + "-modules-after.json")) and len(modules) == 6
    assert modules[0]["Hash"].lower() == "502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f"
    assert modules[1]["Hash"].lower() == "ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e"
    for profile, (width, machine, laa) in PROFILES.items():
        stem = prefix + "-" + profile
        before = load(base / (stem + "-before.json"))
        assert before == load(base / (stem + "-after.json")), stem
        assert (before["profile"], before["machine"], before["large-address-aware"]) == (profile, machine, laa)
        assert len(before["sha256"]) == 64 and before["bytes"] > 0
        assert load(base / (stem + "-source-after.json"))["Hash"].lower() == source_hash
        text = (base / (stem + ".log")).read_text()
        lines = text.splitlines()
        assert lines[0].startswith("PROFILE " + profile + " POINTER_BYTES " + str(width) + " ")
        rows = [(l.split()[0], dict(p.split("=", 1) for p in l.split()[1:])) for l in lines[1:] if "case=" in l]
        assert len(rows) == expected and [number(r, "case", 10) for _, r in rows] == list(range(expected)), stem
        summary = dict(p.split("=", 1) for p in lines[-1].split()[1:])
        assert lines[-1].startswith("SUMMARY ") and number(summary, "cases", 10) == expected
        if "handles-before" in summary:
            assert summary["handles-before"] == summary["handles-after"]
        counts = collections.Counter()
        for kind, row in rows:
            counts[kind] += 1
            if "exception" in row:
                assert number(row, "exception") == 0, (stem, row)
            if "handles-before" in row:
                assert row["handles-before"] == row["handles-after"], (stem, row)
            if kind == "SET":
                assert number(row, "create") == 0 and number(row, "status") == value_status(row), (stem, row)
            elif kind == "CANCEL":
                code = typed(number(row, "kind", 10), number(row, "access"))
                assert number(row, "arm") == code and number(row, "status") == code, (stem, row)
            elif kind == "FAULT":
                code, guards = fault_status(row, width == 4)
                assert number(row, "status") == code, (stem, row)
                assert all(number(row, a + "-guard-after", 10) == guards[a] for a in guards), (stem, row)
            elif kind == "CANCEL-FAULT":
                assert number(row, "status") == typed(number(row, "kind", 10), number(row, "access")), (stem, row)
                assert row["guard-before"] == row["guard-after"], (stem, row)
            elif kind == "TIMING":
                assert number(row, "set") == 0 and number(row, "first") == 0 and number(row, "cancel") == 0
                assert all(number(row, "w" + str(i)) in [0, 0x102] and number(row, "t" + str(i) + "-100ns", 10) >= 0 for i in range(3))
                if number(row, "period") == 0x7FFFFFFFFFFFFFFF:
                    assert all(number(row, "w" + str(i)) == 0x102 for i in range(3))
        if sub == "native-v1":
            assert counts == {"SET": 766, "CANCEL": 60, "STATE": 16}, counts
        if sub == "native-fault-v2":
            assert counts == {"FAULT": 3534, "CANCEL-FAULT": 272}, counts
        report["profiles"][stem] = dict(counts)
        report["captured-cases"] += len(rows)
        report["replayed-records"] += sum(counts[k] for k in ["SET", "CANCEL", "FAULT", "CANCEL-FAULT"])
assert report["captured-cases"] == 18740 and report["replayed-records"] == 18528
print(json.dumps(report, indent=2))
