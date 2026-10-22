#!/usr/bin/env python3
"""Check published native/compiler/runtime receipts without requiring their binaries."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import tarfile


def identity(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def check(root, source=None):
    spec = importlib.util.spec_from_file_location(
        "pbi_native", root / "check_native_evidence.py"
    )
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    native_result = native.check(root)
    proof = json.loads((root / "proof.json").read_bytes())
    assert proof["schema"] == 1
    indexed = {}
    for record in proof["files"]:
        name = record["path"]
        assert (
            name not in indexed
            and not Path(name).is_absolute()
            and ".." not in Path(name).parts
        )
        raw = (root / name).read_bytes()
        assert identity(raw) == {key: record[key] for key in ["bytes", "sha256"]}, name
        indexed[name] = record
    validation = root / "validation"
    projections = json.loads((validation / "public-projections.json").read_bytes())[
        "files"
    ]
    projections = {row["path"]: row for row in projections}

    def receipt(name):
        assert "validation/" + name in indexed
        record = json.loads((validation / name).read_bytes())
        assert (
            record["state"] == "finished"
            and record.get("exit-code", record.get("exit_code", 0)) == 0
        ), name
        for step in record.get("steps", []):
            log = step["log"]
            assert "validation/" + log in indexed
            raw = (validation / log).read_bytes()
            original = {key: step[key] for key in ["bytes", "sha256"]}
            if log in projections:
                projection = projections[log]
                assert original == projection["original"]
                assert identity(raw) == projection["published"]
            else:
                assert identity(raw) == original, log
        return record

    for host, count in [("macos", 8178), ("linux", 8172), ("windows", 7649)]:
        name = (
            "process-basic-candidate1-windows-resume1.json"
            if host == "windows"
            else (
                "process-basic-linux-candidate1-retry1-gates.json"
                if host == "linux"
                else "process-basic-macos-candidate1-gates.json"
            )
        )
        record = receipt(name)
        steps = {step["name"]: step for step in record["steps"]}
        assert {"focus", "full", "capi", "integration", "all-targets"} <= steps.keys()
        for name, step in steps.items():
            assert step["exit-code"] == (
                101 if host == "windows" and name == "full" else 0
            ), (host, name)
        expected = {
            "focus": (2, 0, 0),
            "full": (count, 6 if host == "windows" else 0, 2),
            "capi": (183, 0, 0),
            "integration": (4 if host == "windows" else 544, 0, 0),
        }
        for name, counts in expected.items():
            text = (validation / steps[name]["log"]).read_text()
            matches = re.findall(
                r"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;", text
            )
            assert counts in [tuple(map(int, match)) for match in matches], (host, name)
        if host == "windows":
            baseline = set(
                json.loads(
                    (validation / "mutant-native-baseline-failures.json").read_bytes()
                )["failures"]
            )
            assert (
                set(record["exact-existing-failures"]) == baseline
                and len(baseline) == 5
            )
            additional = "user::windows::wnf::query_tests::native_query_snapshot_captures_typed_state_and_initializes_private_process_state"
            assert set(record["exact-full-failures"]) == baseline | {additional}
            text = (validation / steps["full"]["log"]).read_text()
            failures = set(
                re.findall(r"^    (\S+)\r?$", text[text.rfind("\nfailures:\n") :], re.M)
            )
            assert failures == baseline | {additional}
        else:
            text = (validation / steps["full"]["log"]).read_text()
            assert "2 ignored; 0 measured; 0 filtered out;" in text
    for host in ["macos", "linux"]:
        receipt(f"{host}-process-basic-candidate1-owning-gates-v2.json")
        before = json.loads(
            (
                validation / f"{host}-process-basic-candidate1-archives-before.json"
            ).read_bytes()
        )
        after = json.loads(
            (
                validation / f"{host}-process-basic-candidate1-archives-after.json"
            ).read_bytes()
        )
        assert before["archives"] == after["archives"]
    controls = json.loads(
        (
            validation / "process-basic-linux-native-filesystem-controls-v1.json"
        ).read_bytes()
    )
    assert controls["state"] == "finished" and len(controls["cases"]) == 17
    for case in controls["cases"]:
        assert case["exit-code"] == 0
        assert "test result: ok. 1 passed; 0 failed; 0 ignored;" in case["stdout"]
    failed_linux = json.loads(
        (validation / "process-basic-linux-candidate1-gates.json").read_bytes()
    )
    assert failed_linux["state"] == "failed"
    assert (
        failed_linux["steps"][-1]["name"] == "full"
        and failed_linux["steps"][-1]["exit-code"] == 101
    )
    for name in [
        "process-basic-native-owning-candidate1.json",
        "process-basic-native-ordinary-candidate1.json",
        "process-basic-owning-capi-candidate1.json",
    ]:
        receipt(name)
    compiler = receipt("process-basic-compiler-gates-v1.json")
    assert [step["name"] for step in compiler["steps"]] == ["format", "clippy"]
    assert all(step["exit-code"] == 0 for step in compiler["steps"])
    audit = receipt("process-basic-final-native-audit-v1.json")
    assert audit["verified"] and audit["source-inputs"] == 29139
    assert {row["profile"] for row in audit["native-after"]} == {
        "arm64",
        "x64",
        "x86",
        "x86-laa",
    }
    assert len(audit["native-after"]) == 4
    for row in audit["native-after"]:
        assert row["exit-code"] == 0
        log = row["log"]
        assert "validation/" + log in indexed
        raw = (validation / log).read_bytes()
        assert identity(raw) == {
            key: row["log-identity"][key] for key in ["bytes", "sha256"]
        }
        lines = raw.decode().splitlines()
        assert sum(line.startswith("QUERY ") for line in lines) == 163
        assert sum(line.startswith("FAULT ") for line in lines) == 4800
    frame = json.loads(
        (validation / "process-basic-candidate1-source.json").read_bytes()
    )
    archive = validation / frame["archive"]
    assert identity(archive.read_bytes()) == {
        key: frame[key] for key in ["bytes", "sha256"]
    }
    expected = {row["path"]: row for row in frame["overrides"]}
    assert len(expected) == 10 and len(frame["files"]) == 29139
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        assert (
            len(members) == len(expected)
            and {m.name for m in members} == expected.keys()
        )
        for member in members:
            assert member.isfile() and ".." not in Path(member.name).parts
            assert identity(bundle.extractfile(member).read()) == {
                key: expected[member.name][key] for key in ["bytes", "sha256"]
            }
    if source is not None:
        for row in frame["files"]:
            path = source / row["path"]
            raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
            expected_identity = {key: row[key] for key in ["bytes", "sha256"]}
            if os.name == "nt" and row["path"] == ".cargo/config.toml":
                expected_identity = {
                    "bytes": 374,
                    "sha256": "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
                }
            assert identity(raw) == expected_identity, row["path"]
    return {
        **native_result,
        "compiler-inputs": len(frame["files"]),
        "checked-receipts": 10,
        "repeated-native-calls": 19852,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path)
    args = parser.parse_args()
    print(
        json.dumps(check(Path(__file__).resolve().parent, args.source), sort_keys=True)
    )
