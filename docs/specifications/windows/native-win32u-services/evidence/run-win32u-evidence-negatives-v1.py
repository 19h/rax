from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys

Q = Path(__file__).resolve().parent
D = Path("/Users/int/.assist-native-display-policy-20261022")
SOURCE = D / "docs/specifications/windows/native-win32u-services"
TEMP = D / "t/win32u-evidence-negatives-v1"
assert not TEMP.exists()
TEMP.mkdir()
results = []


def load(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def change(root, filename, function):
    path = root / "evidence" / filename
    value = load(path)
    function(value)
    write(path, value)


def reindex(root):
    path = root / "proof.json"
    proof = load(path)
    for row in proof["files"]:
        raw = (root / row["path"]).read_bytes()
        row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    write(path, proof)


def alter_log(root, filename, record, before, after):
    path = root / "evidence" / filename
    text = path.read_text()
    assert before in text
    path.write_text(text.replace(before, after, 1))
    raw = path.read_bytes()
    receipt = load(root / "evidence" / record)
    for row in receipt["steps"]:
        if row["log"] == filename:
            row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
    write(root / "evidence" / record, receipt)


mutations = [
    (
        "parent",
        lambda r: write(
            r / "proof.json", {**load(r / "proof.json"), "source-parent": "0" * 40}
        ),
    ),
    (
        "source-frame",
        lambda r: change(
            r,
            "win32u-services-candidate1-source.json",
            lambda v: v.update(base="0" * 40),
        ),
    ),
    (
        "native-failures",
        lambda r: change(
            r,
            "win32u-services-candidate2-windows.json",
            lambda v: v["exact-full-failures"].pop(),
        ),
    ),
    (
        "native-test-image",
        lambda r: change(
            r,
            "win32u-services-candidate2-windows.json",
            lambda v: v["test-image-after"].update(sha256="0" * 64),
        ),
    ),
    (
        "guest-false-complete",
        lambda r: change(
            r,
            "win32u-services-owning-capi-candidate3.json",
            lambda v: v["outcomes"][1].update(reason=4),
        ),
    ),
    (
        "owning-archive",
        lambda r: change(
            r,
            "native-win32u-services-candidate1-pre-cpp-archives.json",
            lambda v: v["archives"][0].update(sha256="0" * 64),
        ),
    ),
    (
        "first-entry-state",
        lambda r: alter_log(
            r,
            "native-gdi-init2-entry-v3-four-views-arm64.log",
            "native-gdi-init2-entry-v3-four-views-launch.json",
            "phase=before-first-init2-svc callback=00007fff36bbdb60 table=0000000000000000",
            "phase=before-first-init2-svc callback=00007fff36bbdb60 table=0000000000000001",
        ),
    ),
    (
        "forged-service",
        lambda r: alter_log(
            r,
            "native-win32u-admission-boundary-v2-cases.log",
            "native-win32u-admission-boundary-v2.json",
            'case=file arch=arm64 original-service=0x12fd terminal=Internal("raw Windows arm64 service 0x1fef',
            'case=file arch=arm64 original-service=0x12fd terminal=Internal("native Win32u service NtGdiInit2 0x1fef',
        ),
    ),
    (
        "hidden-original-failure",
        lambda r: change(
            r,
            "win32u-services-macos-candidate1-gates.json",
            lambda v: v.update(state="finished", **{"exit-code": 0}),
        ),
    ),
    (
        "checker-binding",
        lambda r: write(
            r / "proof.json", {**load(r / "proof.json"), "checker-sha256": "0" * 64}
        ),
    ),
    (
        "full-result",
        lambda r: change(
            r,
            "win32u-services-candidate2-windows.json",
            lambda v: next(row for row in v["steps"] if row["name"] == "full").update(
                **{"exit-code": 0}
            ),
        ),
    ),
]
try:
    for name, mutation in [
        ("positive-before", None),
        *mutations,
        ("optimized-O", None),
        ("optimized-OO", None),
        ("positive-after", None),
    ]:
        root = TEMP / name
        shutil.copytree(SOURCE, root)
        if mutation:
            mutation(root)
            reindex(root)
        command = [sys.executable, "-B"]
        if name.startswith("optimized-"):
            command.append("-" + name.split("-", 1)[1])
        command.append(str(root / "check_evidence.py"))
        log = Q / ("win32u-evidence-negative-v1-" + name + ".log")
        with log.open("wb") as output:
            result = subprocess.run(
                command, cwd=D, stdout=output, stderr=subprocess.STDOUT
            )
        expected_pass = name.startswith("positive-")
        assert (result.returncode == 0) == expected_pass, (name, result.returncode)
        raw = log.read_bytes()
        results.append(
            {
                "name": name,
                "expected-pass": expected_pass,
                "exit-code": result.returncode,
                "log": log.name,
                "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest(),
            }
        )
        print(name, result.returncode, flush=True)
    state = {
        "state": "finished",
        "exit-code": 0,
        "outcomes": results,
        "checker-sha256": hashlib.sha256(
            (SOURCE / "check_evidence.py").read_bytes()
        ).hexdigest(),
        "scope": "Eleven reindexed corruptions and both optimized Python modes must reject; two unmodified controls must pass.",
    }
    write(Q / "win32u-evidence-negative-controls-v1.json", state)
finally:
    assert TEMP.resolve().is_relative_to(D.resolve())
    shutil.rmtree(TEMP)
