"""Validate the Cargo-ledger correction, failed trial and native replay results."""

from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import sys

assert not sys.flags.optimize
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def identity(path):
    raw = path.read_bytes()
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def check(directory, root):
    envelope = json.loads((directory / "envelope.json").read_text())
    paths = {
        p.relative_to(directory).as_posix()
        for p in directory.rglob("*")
        if p.is_file() and p.name != "envelope.json"
    }
    assert paths == {row["path"] for row in envelope["files"]}
    for row in envelope["files"]:
        assert identity(directory / row["path"]) == {
            k: row[k] for k in ["bytes", "sha256"]
        }
    v = directory / "validation"
    summary = json.loads((directory / "summary.json").read_text())
    assert (
        summary["product-source-changed"] is False
        and summary["full-process-objective-complete"] is False
    )
    assert (
        summary["corruption-rounds-per-host"]
        == summary["portable-audit-rounds-per-host"]
        == 2
    )
    failed = json.loads((v / "file-attributes-native-stress-epoch3.json").read_text())
    assert failed["state"] == "failed" and failed["steps"][0]["exit-code"] == 1
    assert (
        "accepted semantic corruption: frozen-source-substituted"
        in (v / failed["steps"][0]["log"]).read_text()
    )
    for name in [
        "file-attributes-macos-stress-epoch4.json",
        "file-attributes-linux-stress-epoch4.json",
        "file-attributes-native-stress-epoch4.json",
    ]:
        result = json.loads((v / name).read_text())
        assert (
            result["state"] == "finished"
            and result["exit-code"] == 0
            and len(result["steps"]) == 2
        )
        for row in result["steps"]:
            assert row["exit-code"] == 0
            assert identity(v / row["log"]) == {k: row[k] for k in ["bytes", "sha256"]}
            outcome = json.loads((v / row["log"]).read_text())
            assert (
                outcome["all-rejected"]
                and outcome["positive-restorations-verified"]
                == len(outcome["controls"])
                == 13
            )
            assert all(x["rejected"] for x in outcome["controls"])
    audits = json.loads((v / "file-attributes-portable-audits-epoch4.json").read_text())
    assert audits["state"] == "finished" and audits["exit-code"] == 0
    assert {(x["host"], x["round"]) for x in audits["steps"]} == {
        (h, n) for h in ["macos", "linux", "windows"] for n in [1, 2]
    }
    for row in audits["steps"]:
        assert row["exit-code"] == 0 and identity(v / row["log"]) == {
            k: row[k] for k in ["bytes", "sha256"]
        }
        outcome = json.loads((v / row["log"]).read_text())
        assert outcome["state"] == "verified" and outcome[
            "native-retained-bytes-verified"
        ] == (row["host"] == "windows")
    main = root / "docs/specifications/windows/native-file-attributes"
    assert (
        identity(main / "validation-envelope.json")["sha256"]
        == summary["corrected-main-envelope-sha256"]
        == audits["envelope-sha256"]
    )
    spec = importlib.util.spec_from_file_location(
        "corrected_attributes", main / "check_validation.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    result = module.audit(main, root)
    return {
        "state": "verified",
        "corruption-runs": 6,
        "rejected-corruptions": 78,
        "positive-restorations": 78,
        "product-source-changed": False,
        "native-retained-bytes-verified": result["native-retained-bytes-verified"],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=Path, default=HERE)
    parser.add_argument("--source-root", type=Path, default=ROOT)
    args = parser.parse_args()
    print(json.dumps(check(args.directory, args.source_root), indent=2))
