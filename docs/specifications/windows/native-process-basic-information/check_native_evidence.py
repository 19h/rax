#!/usr/bin/env python3
"""Verify recorded producer identities, native observations and optional live files."""

import argparse
import hashlib
import json
from pathlib import Path
import sys


def identity(path):
    data = path.read_bytes()
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def fields(line):
    return dict(item.split("=", 1) for item in line.split()[1:])


def check(root, live=False):
    if sys.flags.optimize:
        raise RuntimeError("Evidence checks require Python assertions; omit -O/-OO")
    native = root / "native"
    catalog = json.loads((root / "native-files.json").read_bytes())
    assert sorted(p.name for p in native.iterdir()) == sorted(catalog)
    for name, expected in catalog.items():
        assert identity(native / name) == expected, name
    total = 0
    for version, expected_calls in [(5, 163), (6, 4963)]:
        stem = f"native-process-basic-information-20261022-v{version}"
        launch = json.loads((native / f"{stem}-four-views-launch.json").read_bytes())
        assert launch["state"] == "finished" and launch["exit-code"] == 0
        assert launch["source-before"] == launch["source-after"]
        assert identity(native / f"{stem}.cpp") == {
            key: launch["source-before"][key] for key in ["bytes", "sha256"]
        }
        assert launch["runtime-modules-before"] == launch["runtime-modules-after"]
        assert len(launch["runtime-modules-before"]) == 8
        assert {o["profile"] for o in launch["outcomes"]} == {
            "arm64",
            "x64",
            "x86",
            "x86-laa",
        }
        for step in launch["steps"]:
            assert step["exit-code"] == 0, step["name"]
            assert identity(native / step["log"]) == {
                key: step[key] for key in ["bytes", "sha256"]
            }
        for outcome in launch["outcomes"]:
            profile = outcome["profile"]
            assert outcome["exit-code"] == 0
            assert outcome["executable-before"] == outcome["executable-after"]
            lines = (native / outcome["log"]).read_text().splitlines()
            meta = fields(lines[0])
            width = 4 if profile.startswith("x86") else 8
            assert meta["profile"] == profile
            assert int(meta["pointer-bytes"]) == width
            assert int(meta["basic-bytes"]) == 6 * width
            assert int(meta["extended-candidate-bytes"]) == 8 * width
            queries = [fields(s) for s in lines if s.startswith("QUERY ")]
            faults = [fields(s) for s in lines if s.startswith("FAULT ")]
            assert len(queries) == 163
            assert len(faults) == (4800 if version == 6 else 0)
            for query in queries + faults:
                assert query["exception"] == "00000000"
                assert query["last-error"] == "12345678"
            for query in queries:
                assert len(bytes.fromhex(query["bytes"])) == 128
            if version == 6:
                seen = set()
                for fault in faults:
                    key = (int(fault["id"]), fault["pattern"])
                    assert key not in seen
                    seen.add(key)
                    for name, size in [
                        ("out-head", 128),
                        ("out-tail", 128),
                        ("ret-head", 32),
                        ("ret-tail", 16),
                    ]:
                        assert len(bytes.fromhex(fault[name])) == size
                assert seen == {
                    (number, pattern)
                    for number in range(1, 2401)
                    for pattern in ["A5", "5A"]
                }
            summary = fields(next(s for s in lines if s.startswith("SUMMARY ")))
            assert int(summary["calls"]) == expected_calls
            assert summary["opened"] == summary["closed"] == "9"
            assert summary["handles-before"] == summary["handles-after"]
            total += expected_calls
        if live:
            records = [launch["source-after"]] + launch["runtime-modules-after"]
            records += [o["executable-after"] for o in launch["outcomes"]]
            for record in records:
                assert identity(Path(record["path"])) == {
                    key: record[key] for key in ["bytes", "sha256"]
                }, record["path"]
    assert total == 20504
    return {"native-calls": total, "replayed-v6-calls": 19852, "live-native": live}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live-native", action="store_true")
    args = parser.parse_args()
    print(
        json.dumps(
            check(Path(__file__).resolve().parent, args.live_native), sort_keys=True
        )
    )
