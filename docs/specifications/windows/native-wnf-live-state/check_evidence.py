#!/usr/bin/env python3
"""Verify WNF live-state correction receipts, controls and frozen source."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import tarfile


def identity(data):
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def check(root, source=None):
    if sys.flags.optimize:
        raise RuntimeError("Evidence verification requires assertions; omit -O/-OO")
    proof = json.loads((root / "proof.json").read_bytes())
    assert proof["schema"] == 1
    indexed = {}
    for row in proof["files"]:
        name = row["path"]
        assert (
            name not in indexed
            and not Path(name).is_absolute()
            and ".." not in Path(name).parts
        )
        assert identity((root / name).read_bytes()) == {
            key: row[key] for key in ["bytes", "sha256"]
        }, name
        indexed[name] = row
    assert {
        "evidence/" + p.name for p in (root / "evidence").iterdir()
    } == indexed.keys()
    evidence = root / "evidence"

    def load(name):
        assert "evidence/" + name in indexed
        return json.loads((evidence / name).read_bytes())

    def steps(record):
        for row in record["steps"]:
            assert "evidence/" + row["log"] in indexed
            assert identity((evidence / row["log"]).read_bytes()) == {
                key: row[key] for key in ["bytes", "sha256"]
            }

    parent = load("wnf-live-parent-native-v1.json")
    assert parent["state"] == "finished" and parent["exit-code"] == 101
    assert "0 passed; 1 failed; 0 ignored;" in parent["stdout"]
    unix = load("wnf-live-unix-gates-v1.json")
    assert unix["state"] == "finished" and unix["exit-code"] == 0
    steps(unix)
    for host in ["macos", "linux"]:
        selected = [row for row in unix["steps"] if row["host"] == host]
        assert {"module", "all-targets", "ci-runner"} <= {
            row["name"] for row in selected
        }
        assert all(row["exit-code"] == 0 for row in selected)
        module = next(row for row in selected if row["name"] == "module")
        assert any(
            "12 passed; 0 failed; 0 ignored;" in value for value in module["results"]
        )
    failed = load("wnf-live-native-candidate1.json")
    assert failed["state"] == "failed" and failed["step"] == "ci-runner"
    assert [row["exit-code"] for row in failed["steps"]] == [0, 0, 1]
    steps(failed)
    assert (
        "No module named '_overlapped'"
        in (evidence / failed["steps"][-1]["log"]).read_text()
    )
    native = load("wnf-live-native-candidate1-resume1.json")
    assert native["state"] == "finished" and native["exit-code"] == 0
    assert all(row["exit-code"] == 0 for row in native["steps"])
    steps(native)
    assert native["python-before"] == native["python-after"]
    assert native["overlapped-before"] == native["overlapped-after"]
    focus = next(row for row in native["steps"] if row["name"] == "focus")
    assert any("14 passed; 0 failed; 0 ignored;" in value for value in focus["results"])
    assert "wnf-native-system attempt=" in (evidence / focus["log"]).read_text()
    repeat = load("wnf-live-native-repeat-v2.json")
    assert repeat["state"] == "finished" and len(repeat["repeats"]) == 12
    assert repeat["candidate-before"] == repeat["candidate-after"]
    assert repeat["parent-before"] == repeat["parent-after"]
    assert repeat["parent-result"]["exit-code"] == 101
    assert repeat["parent-before"]["sha256"] == parent["image"]["sha256"]
    for row in repeat["repeats"]:
        assert row["exit-code"] == 0
        assert "ok. 1 passed; 0 failed; 0 ignored;" in row["stdout"]
        assert "wnf-native-system attempt=" in row["stdout"]
    manifest = load("wnf-live-candidate1-source.json")
    assert manifest["base"] == "d5d8ea486c1c4e1b8be8d80d0be7698fa7200b02"
    assert len(manifest["files"]) == 29139
    archive = evidence / manifest["archive"]
    assert identity(archive.read_bytes()) == {
        key: manifest[key] for key in ["bytes", "sha256"]
    }
    expected = {row["path"]: row for row in manifest["overrides"]}
    assert set(expected) == {
        "src/user/windows/wnf/query_tests.rs",
        "tools/ci/process_embedding.py",
    }
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        assert len(members) == 2 and {m.name for m in members} == expected.keys()
        for member in members:
            assert member.isfile()
            assert identity(bundle.extractfile(member).read()) == {
                key: expected[member.name][key] for key in ["bytes", "sha256"]
            }
    if source is not None:
        for row in manifest["files"]:
            path = source / row["path"]
            data = (
                os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
            )
            expected = {key: row[key] for key in ["bytes", "sha256"]}
            if os.name == "nt" and row["path"] == ".cargo/config.toml":
                expected = {
                    "bytes": 374,
                    "sha256": "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
                }
            assert identity(data) == expected, row["path"]
    return {
        "module-tests": {"macos": 12, "linux": 12, "windows": 14},
        "native-repeat-runs": 12,
        "parent-exit-code": 101,
        "compiler-inputs": 29139,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path)
    args = parser.parse_args()
    print(
        json.dumps(check(Path(__file__).resolve().parent, args.source), sort_keys=True)
    )
