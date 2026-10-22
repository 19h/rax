"""Exercise semantic evidence rejection after recomputing its outer inventory."""

from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys
import traceback

Q = Path(__file__).resolve().parent
R = Path("/Users/int/.assist-native-display-policy-20261022")
D = R / "docs/specifications/windows/native-read-virtual-memory"
T = R / "target/read-memory-evidence-negative-v1"
F = Q / "read-memory-evidence-negative-controls-v1.json"
assert not T.exists() and not F.exists()
state = {
    "state": "preparing",
    "cases": [],
    "checker-sha256": hashlib.sha256(
        (D / "check_evidence.py").read_bytes()
    ).hexdigest(),
}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def run(label, expected=None, flags=()):
    argv = [
        sys.executable,
        *flags,
        "-B",
        str(D / "check_evidence.py"),
        "--evidence-dir",
        str(T),
        "--source-root",
        str(R),
    ]
    child = subprocess.run(
        argv, cwd=R, stdout=subprocess.PIPE, stderr=subprocess.STDOUT
    )
    text = child.stdout.decode(errors="strict")
    if expected is None:
        assert child.returncode == 0, text
    else:
        assert child.returncode != 0 and expected in text, (
            label,
            child.returncode,
            text,
        )
    p = Q / ("read-memory-evidence-negative-v1-" + label + ".log")
    assert not p.exists()
    p.write_bytes(child.stdout)
    state["cases"].append(
        {
            "name": label,
            "expected-rejection": expected,
            "exit-code": child.returncode,
            "log": p.name,
            "bytes": len(child.stdout),
            "sha256": hashlib.sha256(child.stdout).hexdigest(),
        }
    )
    save()
    print(label, child.returncode, flush=True)


def index():
    result = subprocess.run(
        [sys.executable, "-B", str(T / "build_proof.py")], cwd=R, capture_output=True
    )
    assert result.returncode == 0, result.stderr


def mutate_json(label, name, change, expected):
    p = T / "evidence" / name
    original = p.read_bytes()
    document = json.loads(original)
    change(document)
    p.write_text(json.dumps(document, indent=2) + "\n")
    index()
    try:
        run(label, expected)
    finally:
        p.write_bytes(original)
        index()


save()
try:
    shutil.copytree(D, T)
    state["state"] = "running"
    save()
    run("baseline-positive")
    original = (T / "proof.json").read_bytes()
    proof = json.loads(original)
    proof["base"] = "0" * 40
    (T / "proof.json").write_text(json.dumps(proof, indent=2) + "\n")
    run("parent", "parent/schema")
    (T / "proof.json").write_bytes(original)

    p = next((T / "native").glob("*v7-four-views-arm64.log"))
    original = p.read_bytes()
    rows = original.decode().splitlines(keepends=True)
    at = next(i for i, line in enumerate(rows) if line.startswith("{"))
    row = json.loads(rows[at])
    row["status"] ^= 1
    rows[at] = json.dumps(row) + "\n"
    p.write_text("".join(rows))
    index()
    run("native-status", "native fixture is exact compiled input:")
    p.write_bytes(original)
    index()

    mutate_json(
        "source-frame",
        "read-memory-candidate5-source.json",
        lambda d: d.update(base="0" * 40),
        "compiler frame digest",
    )
    mutate_json(
        "windows-failures",
        "read-memory-candidate5-windows.json",
        lambda d: d["exact-full-failures"].__setitem__(0, "unrecorded_failure"),
        "Windows exact remaining failures",
    )
    mutate_json(
        "windows-image",
        "read-memory-candidate5-windows.json",
        lambda d: d["test-image-after"].update(sha256="0" * 64),
        "Windows exact test image",
    )
    mutate_json(
        "owning-archive",
        "macos-read-memory-candidate5-archives-after.json",
        lambda d: d["archives"][0].update(sha256="0" * 64),
        "actual owning archives stable",
    )
    mutate_json(
        "guest-completion",
        "read-memory-owning-capi-candidate5.json",
        lambda d: d["outcomes"][1].update(reason=4),
        "C API next frontier, not false guest exit",
    )
    mutate_json(
        "native-repeat",
        "native-read-repeat-v3.json",
        lambda d: d["steps"][0].update(**{"complete-nonresidency-row-match": False}),
        "alignment repeat execution",
    )
    mutate_json(
        "quality-exit",
        "read-memory-quality-v4.json",
        lambda d: d["steps"][1].update(**{"exit-code": 1}),
        "formatter/Clippy exit",
    )
    run("optimized-python", "optimized Python is not an evidence checker", ("-O",))
    run(
        "double-optimized-python",
        "optimized Python is not an evidence checker",
        ("-OO",),
    )
    run("restored-positive")
    assert T.resolve().parent == (R / "target").resolve()
    shutil.rmtree(T)
    state.update(
        state="finished",
        **{
            "exit-code": 0,
            "temporary-copy-removed": not T.exists(),
            "production-source-edits": 0,
        }
    )
except BaseException:
    state.update(state="failed", error=traceback.format_exc(), **{"exit-code": 1})
finally:
    save()
raise SystemExit(state["exit-code"])
