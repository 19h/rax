#!/usr/bin/env python3
"""Verify fixed native provisioning oracles, complete gates and frozen source."""
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
        assert (
            name not in indexed
            and not Path(name).is_absolute()
            and ".." not in Path(name).parts
        )
        assert identity((root / name).read_bytes()) == {
            k: row[k] for k in ["bytes", "sha256"]
        }, name
        indexed[name] = row
    evidence = root / "evidence"
    assert {"evidence/" + p.name for p in evidence.iterdir()} == indexed.keys()

    def load(name):
        assert "evidence/" + name in indexed
        return json.loads((evidence / name).read_bytes())

    def log(row):
        name = row["log"]
        assert "evidence/" + name in indexed
        raw = (evidence / name).read_bytes()
        assert identity(raw) == {k: row[k] for k in ["bytes", "sha256"]}
        return raw.decode(errors="replace")

    native = None
    for host, focus, full in [
        ("macos", 12, 8184),
        ("linux", 12, 8179),
        ("windows", 14, 7655),
    ]:
        name = (
            "wnf-private-candidate1-windows.json"
            if host == "windows"
            else "wnf-private-" + host + "-candidate1-gates.json"
        )
        record = load(name)
        assert (
            record["state"] == "finished"
            and record["exit-code"] == 0
            and record["source-inputs"] == 29141
        )
        by_name = {row["name"]: row for row in record["steps"]}
        assert {"focus", "full", "capi", "integration", "all-targets"} <= by_name.keys()
        for row in record["steps"]:
            output = log(row)
            assert row["results"] == re.findall(r"test result: ([^\r\n]+)", output)
            assert row["exit-code"] == (
                101 if host == "windows" and row["name"] == "full" else 0
            )
        for gate, count in [
            ("focus", focus),
            ("capi", 183),
            ("integration", 4 if host == "windows" else 544),
        ]:
            assert any(
                f"{count} passed; 0 failed;" in value
                for value in by_name[gate]["results"]
            )
        failed = 5 if host == "windows" else 0
        assert any(
            f"{full} passed; {failed} failed; 2 ignored; 0 measured; 0 filtered out;"
            in value
            for value in by_name["full"]["results"]
        )
        if host == "windows":
            native = record
            expected = load("mutant-native-baseline-failures.json")["failures"]
            assert (
                len(expected) == 5 and sorted(expected) == record["exact-full-failures"]
            )
            output = log(by_name["full"])
            actual = re.findall(
                r"^    (\S+)\r?$", output[output.rfind("\nfailures:\n") :], re.M
            )
            assert sorted(actual) == sorted(expected)
            assert record["test-image-before"] == record["test-image-after"]
            focus_output = log(by_name["focus"])
            assert (
                "private-provisioned=true private-metadata-bytes=72" in focus_output
                and "oracle=Data" in focus_output
            )
    probe = load("native-wnf-provisioning-probe-v1.json")
    assert (
        probe["state"] == "finished"
        and probe["machine"] == "ARM64"
        and "29683" in probe["platform"]
    )
    assert probe["registry-before"] == probe["registry-after"]
    rows = {row["name"]: row for row in probe["registry-before"]}
    private = rows["41911A28A3BC10B5"]
    assert private["present"] and private["kind"] == 3 and private["bytes"] == 72
    assert identity(bytes.fromhex(private["data-hex"])) == {
        k: private[k] for k in ["bytes", "sha256"]
    }
    assert (
        not rows["0000000000000000"]["present"]
        and rows["0000000000000000"]["status"] == 2
    )
    direct = {row["name"]: row for row in probe["queries"]}
    assert direct["41911A28A3BC10B5"] == {
        "name": "41911A28A3BC10B5",
        "status": "0x0",
        "change-stamp": 0,
        "returned-bytes": 0,
        "payload-hex": None,
        "query-capacity-bytes": 0,
    }
    assert identity(
        (evidence / "native-wnf-provisioning-probe-v1.py").read_bytes()
    ) == {k: probe["source"][k] for k in ["bytes", "sha256"]}
    parent = load("wnf-private-parent-x64-ci-v1.json")
    assert (
        parent["source-commit"] == "945ace5d5de706da96418c8b939ede07c0166ae1"
        and parent["conclusion"] == "failure"
    )
    output = log(parent)
    assert parent["source-commit"] in output
    assert "Microsoft Windows Server 2022" in output and "10.0.20348" in output
    assert (
        "query_tests.rs:287" in output and "13 passed; 1 failed; 0 ignored;" in output
    )
    assert "called `Option::unwrap()` on a `None` value" in output
    quality = load("wnf-private-quality-v1.json")
    assert quality["state"] == "finished"
    assert {row["name"] for row in quality["steps"]} == {"rustfmt", "clippy"}
    for row in quality["steps"]:
        log(row)
        assert row["exit-code"] == 0
    manifest = load("wnf-private-candidate1-source.json")
    assert (
        manifest["base"] == parent["source-commit"] and len(manifest["files"]) == 29141
    )
    expected = {row["path"]: row for row in manifest["overrides"]}
    assert set(expected) == {"src/user/windows/wnf/query_tests.rs"}
    archive = evidence / manifest["archive"]
    assert identity(archive.read_bytes()) == {
        k: manifest[k] for k in ["bytes", "sha256"]
    }
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        assert len(members) == 1 and members[0].name in expected and members[0].isfile()
        assert identity(bundle.extractfile(members[0]).read()) == {
            k: expected[members[0].name][k] for k in ["bytes", "sha256"]
        }
    audit = load("wnf-private-source-audit-v1.json")
    assert audit["state"] == "finished" and audit["parent"] == manifest["base"]
    assert (
        audit["inherited-inputs-matching-parent"] == 29140
        and audit["overlay-members"] == 1
        and audit["full-input-hashes-verified"]
    )
    assert (
        audit["tracked-diff"] == ["src/user/windows/wnf/query_tests.rs"]
        and audit["capi-header-diff"] == ""
    )
    reference = json.loads((root / "reference-provenance.json").read_bytes())
    for row in reference["files"]:
        assert identity((root / row["path"]).read_bytes()) == {
            k: row[k] for k in ["bytes", "sha256"]
        }
    if source is not None:
        for row in manifest["files"]:
            path = source / row["path"]
            raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
            wanted = {k: row[k] for k in ["bytes", "sha256"]}
            if os.name == "nt" and row["path"] == ".cargo/config.toml":
                wanted = {
                    "bytes": 374,
                    "sha256": "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
                }
            assert identity(raw) == wanted, row["path"]
    return {
        "compiler-inputs": 29141,
        "overlay-files": 1,
        "native-private-provisioned": True,
        "module-tests": {"macos": 12, "linux": 12, "windows": 14},
        "windows-full-failures": 5,
        "x64-independent-provisioning": "not yet observed",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path)
    args = parser.parse_args()
    print(
        json.dumps(check(Path(__file__).resolve().parent, args.source), sort_keys=True)
    )
