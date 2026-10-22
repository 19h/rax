#!/usr/bin/env python3
"""Verify procfs lifetime controls, frozen sources and platform receipts."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tarfile


def identity(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def check(root, source=None):
    if sys.flags.optimize:
        raise RuntimeError("Evidence verification requires assertions; omit -O/-OO")
    proof = json.loads((root / "proof.json").read_bytes())
    assert proof["schema"] == 1
    indexed = {}
    for row in proof["files"]:
        name = row["path"]
        assert name not in indexed
        assert not Path(name).is_absolute() and ".." not in Path(name).parts
        assert identity((root / name).read_bytes()) == {
            k: row[k] for k in ["bytes", "sha256"]
        }, name
        indexed[name] = row
    evidence = root / "evidence"
    assert {"evidence/" + p.name for p in evidence.iterdir()} == indexed.keys()

    def load(name):
        assert "evidence/" + name in indexed
        return json.loads((evidence / name).read_bytes())

    def log(row, key="log"):
        name = row[key]
        assert "evidence/" + name in indexed
        raw = (evidence / name).read_bytes()
        assert identity(raw) == {k: row[k] for k in ["bytes", "sha256"]}
        return raw.decode(errors="replace")

    def steps(record, failures=()):
        assert record["state"] == "finished" and record["exit-code"] == 0
        for row in record["steps"]:
            text = log(row)
            if "results" in row:
                assert row["results"] == re.findall(r"test result: ([^\r\n]+)", text)
            assert row["exit-code"] == (101 if row.get("name") in failures else 0)

    references = json.loads((root / "reference-provenance.json").read_bytes())
    for row in references["files"]:
        assert identity((root / row["path"]).read_bytes()) == {
            k: row[k] for k in ["bytes", "sha256"]
        }

    parent = load("parent-collision-v1.json")
    assert parent["state"] == "finished" and parent["exit-code"] == 101
    assert parent["baseline"] == "e621a832adb9e9dcbe4173f8e467571f2e6b34c1"
    for key in ["test-overlay", "log"]:
        row = parent[key]
        assert identity((evidence / row["file"]).read_bytes()) == {
            k: row[k] for k in ["bytes", "sha256"]
        }
    raw = (evidence / parent["log"]["file"]).read_text()
    assert "left: 3" in raw and "right: -2" in raw
    assert "0 passed; 1 failed; 0 ignored;" in raw

    for host, focus, full in [("macos", 8, 8184), ("linux", 9, 8179)]:
        record = load("procfs-" + host + "-candidate1-gates.json")
        steps(record)
        by_name = {row["name"]: row for row in record["steps"]}
        assert set(by_name) == {"focus", "full", "capi", "integration", "all-targets"}
        for name, count in [
            ("focus", focus),
            ("full", full),
            ("capi", 183),
            ("integration", 544),
        ]:
            assert any(
                f"{count} passed; 0 failed;" in value
                for value in by_name[name]["results"]
            ), (host, name)
        assert any(
            "2 ignored; 0 measured; 0 filtered out;" in value
            for value in by_name["full"]["results"]
        )
        if host == "linux":
            assert (
                "an_exited_guest_tid_never_opens_a_colliding_live_host_thread ... ok"
                in log(by_name["focus"])
            )

    native = load("procfs-candidate1-windows.json")
    steps(native, failures={"full"})
    by_name = {row["name"]: row for row in native["steps"]}
    assert {
        "test-build",
        "focus",
        "full",
        "capi",
        "integration",
        "all-targets",
    } <= by_name.keys()
    for name, count in [("focus", 6), ("capi", 183), ("integration", 4)]:
        assert any(
            f"{count} passed; 0 failed;" in value for value in by_name[name]["results"]
        )
    assert any(
        "7655 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;" in value
        for value in by_name["full"]["results"]
    )
    expected = load("mutant-native-baseline-failures.json")["failures"]
    assert len(expected) == 5 and sorted(expected) == native["exact-full-failures"]
    output = log(by_name["full"])
    failures = re.findall(
        r"^    (\S+)\r?$", output[output.rfind("\nfailures:\n") :], re.M
    )
    assert sorted(failures) == sorted(expected)
    assert native["test-image-before"] == native["test-image-after"]

    for host in ["macos", "linux", "windows"]:
        record = load("procfs-" + host + "-user-linux-v1.json")
        steps(record)
        assert {r["name"] for r in record["steps"]} == (
            {"toolchain", "integration"} if host == "windows" else {"integration"}
        )
        selected = next(r for r in record["steps"] if r["name"] == "integration")
        if host == "windows":
            assert any(
                "0 passed; 0 failed; 0 ignored;" in value
                for value in selected["results"]
            )
        else:
            assert any(
                "42 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out;" in value
                for value in selected["results"]
            )

    for host in ["macos", "linux", "windows"]:
        if host == "windows":
            owner = load("procfs-native-owning-candidate1.json")
            before = load("native-procfs-candidate1-pre-cpp-archives.json")
            after = load(
                "native-procfs-candidate1-final-owning-archive-identities.json"
            )
        else:
            owner = load(host + "-procfs-candidate1-owning-gates-v2.json")
            before = load(host + "-procfs-candidate1-archives-before.json")
            after = load(host + "-procfs-candidate1-archives-after.json")
        steps(owner)
        assert owner["source-inputs"] == 3974
        assert before["archives"] == after["archives"] and len(before["archives"]) == 2
        assert before["core-features"] == [] and after["core-features"] == []
        for archive in before["archives"]:
            assert (
                archive["structural-member-walk"] == "pass" and archive["members"] > 0
            )
        if host == "macos":
            ctest = next(row for row in owner["steps"] if row["gate"] == "ctest")
            text = log(ctest)
            assert re.search(r"100% tests passed(?:, 0 tests failed)? out of 7", text)
            assert len(re.findall(r"^\d+/7 Test .*? Passed\s", text, re.M)) == 7
        else:
            cpp = next(
                row
                for row in owner["steps"]
                if row.get("name", row.get("gate")) == "cpp"
            )
            output = log(cpp)
            assert (
                "linked-archive-before" in output and "linked-archive-after" in output
            )

    formatting = load("procfs-rustfmt-owned-v1.json")
    assert formatting["exit-code"] == 0 and len(formatting["owned"]) == 8
    log(formatting)
    quality = load("procfs-quality-gates-v1.json")
    assert quality["state"] == "finished"
    for row in quality["steps"]:
        log(row)
    q = {row["name"]: row for row in quality["steps"]}
    assert q["clippy-all-targets"]["exit-code"] == 0
    assert q["clippy-library-strict"]["exit-code"] == 0
    assert q["clippy-all-targets-strict"]["exit-code"] != 0

    manifest = load("procfs-candidate1-source.json")
    assert manifest["base"] == parent["baseline"] and len(manifest["files"]) == 29141
    archive = evidence / manifest["archive"]
    assert identity(archive.read_bytes()) == {
        k: manifest[k] for k in ["bytes", "sha256"]
    }
    expected = {row["path"]: row for row in manifest["overrides"]}
    assert len(expected) == 8
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        assert len(members) == 8 and {m.name for m in members} == expected.keys()
        for member in members:
            assert member.isfile()
            assert identity(bundle.extractfile(member).read()) == {
                k: expected[member.name][k] for k in ["bytes", "sha256"]
            }
    audit = load("procfs-source-audit-v1.json")
    assert audit["state"] == "finished" and audit["parent"] == parent["baseline"]
    assert audit["inherited-inputs-matching-parent"] == 29133
    assert audit["full-input-hashes-verified"] and audit["capi-header-diff"] == ""
    assert set(audit["tracked-diff"] + audit["new-files"]) == expected.keys()
    original = load("procfs-owning-source-hashes-candidate1.json")
    corrected = load("procfs-owning-source-hashes-candidate1-v2.json")
    assert (
        original["sources"] == corrected["sources"]
        and original["helpers"] == corrected["helpers"]
    )
    assert len(corrected["sources"]) == 3974
    assert corrected["source-parent-rax"] == manifest["base"]
    assert (
        corrected["source-freeze-sha256"]
        == identity((evidence / "procfs-candidate1-source.json").read_bytes())["sha256"]
    )
    if source is not None:
        for row in manifest["files"]:
            path = source / row["path"]
            raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
            expected = {k: row[k] for k in ["bytes", "sha256"]}
            if os.name == "nt" and row["path"] == ".cargo/config.toml":
                expected = {
                    "bytes": 374,
                    "sha256": "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
                }
            assert identity(raw) == expected, row["path"]
    return {
        "compiler-inputs": 29141,
        "owning-inputs": 3974,
        "focus": {"linux": 9, "macos": 8, "windows": 6},
        "windows-full-failures": 5,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path)
    args = parser.parse_args()
    print(
        json.dumps(check(Path(__file__).resolve().parent, args.source), sort_keys=True)
    )
