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
D = R / "docs/specifications/windows/native-gre-initialize"
T = R / "target/gre-initialize-evidence-negative-v1"
F = Q / "gre-initialize-evidence-negative-controls-v1.json"
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
    p = Q / ("gre-initialize-evidence-negative-v1-" + label + ".log")
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
    native = T / "native/native-gre-initialize-v1-four-views-arm64.log"
    launch = T / "evidence/native-gre-initialize-v1-four-views-launch.json"
    raw_native = native.read_bytes()
    raw_launch = launch.read_bytes()
    lines = raw_native.decode().splitlines(keepends=True)
    at = next(i for i, line in enumerate(lines) if line.startswith("OPEN "))
    lines[at] = lines[at].replace("status=C0000022", "status=00000000")
    native.write_bytes("".join(lines).encode())
    data = json.loads(raw_launch)
    row = next(row for row in data["steps"] if row["name"] == "arm64")
    row.update(
        bytes=native.stat().st_size,
        sha256=hashlib.sha256(native.read_bytes()).hexdigest(),
    )
    launch.write_text(json.dumps(data, indent=2) + "\n")
    index()
    try:
        run("native-status", "exact compiled fixture")
    finally:
        native.write_bytes(raw_native)
        launch.write_bytes(raw_launch)
        index()
    mutate_json(
        "source-frame",
        "gre-initialize-candidate1-source.json",
        lambda d: d.update(base="0" * 40),
        "compiler frame digest",
    )
    mutate_json(
        "windows-failures",
        "gre-initialize-candidate1-windows.json",
        lambda d: d["exact-full-failures"].__setitem__(0, "unrecorded_failure"),
        "Windows exact remaining failures",
    )
    mutate_json(
        "windows-image",
        "gre-initialize-candidate1-windows.json",
        lambda d: d["test-image-after"].update(sha256="0" * 64),
        "Windows exact test executable",
    )
    mutate_json(
        "owning-archive",
        "macos-gre-initialize-candidate1-archives-after.json",
        lambda d: d["archives"][0].update(sha256="0" * 64),
        "actual owning archives stable",
    )
    mutate_json(
        "guest-completion",
        "gre-initialize-owning-capi-candidate1.json",
        lambda d: d["outcomes"][1].update(reason=4),
        "C API next frontier is not false guest exit",
    )
    mutate_json(
        "native-repeat",
        "native-gre-repeat-v1.json",
        lambda d: d["steps"][0].update(**{"read-tree-records-match": False}),
        "repeat native fields",
    )
    mutate_json(
        "quality-exit",
        "gre-initialize-quality-v1.json",
        lambda d: d["steps"][1].update(**{"exit-code": 1}),
        "formatter/Clippy result",
    )
    mutate_json(
        "retain-mac-failure",
        "gre-initialize-macos-candidate1-gates.json",
        lambda d: d.update(state="finished"),
        "retained Mac failed run",
    )
    mutate_json(
        "parent-failure",
        "native-gre-parent-regression-v1.json",
        lambda d: d["steps"][-1].update(**{"exit-code": 0}),
        "compiled expected parent failure",
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
