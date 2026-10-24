"""Check primary projection integrity and semantic rejection under optimized Python."""

from pathlib import Path, PureWindowsPath
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

N = Path(__file__).resolve().parent
D = Path("/Users/int/.assist-native-display-policy-20261022")
result = {"positive": [], "negative": []}
record = N / "remote-connect-projection-controls-v2.json"
assert not record.exists()


def identity(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def reindex(root, relative, name, mutate):
    folder = root / relative
    path = folder / name
    raw = mutate(path.read_bytes())
    path.write_bytes(raw)
    receipt = folder / (
        "receipt.json"
        if relative.startswith("native-remote-connect-state")
        else "native-remote-connect-cold-v1.json"
    )
    data = json.loads(receipt.read_bytes())
    matched = []
    for step in data["steps"]:
        item = step["log"]
        native = item.get("file", item.get("path"))
        if native.replace("\\", "/").endswith("/" + name):
            item.update(identity(raw))
            matched.append(step)
    assert len(matched) == 1, name
    receipt.write_text(json.dumps(data, indent=2) + "\n")


def mutate(root, case):
    fixed = "native-remote-connect-state-v2-bundle-v1"
    cold = "native-remote-cold-bundle-v1"
    if case == "source-change":
        p = root / "native-remote-connect-state-v2.cpp"
        p.write_bytes(p.read_bytes() + b"\n// changed\n")
    elif case == "log-change-unindexed":
        p = root / fixed / "arm64/native-arm64.log"
        p.write_bytes(p.read_bytes().replace(b"value=0", b"value=1", 1))
    elif case == "query-value-reindexed":
        reindex(
            root,
            fixed,
            "arm64/native-arm64.log",
            lambda raw: raw.replace(b"value=0", b"value=1", 1),
        )
    elif case == "query-coverage-reindexed":
        reindex(
            root,
            fixed,
            "arm64/native-arm64.log",
            lambda raw: raw.replace(b"sample=1 ", b"sample=0 ", 1),
        )
    elif case == "callback-index-reindexed":
        reindex(
            root,
            cold,
            "native-remote-connect-cold-v1-normal.log",
            lambda raw: raw.replace(b"index=84", b"index=85", 1),
        )
    elif case == "failure-marker-reindexed":
        reindex(
            root,
            cold,
            "native-remote-connect-cold-v1-callback-failure.log",
            lambda raw: raw.replace(
                b"cold-query-return value=0 last_error=12345678 win32-thread-present=0",
                b"cold-query-return value=0 last_error=12345678 win32-thread-present=1",
                1,
            ),
        )
    elif case in ("executable-change", "runtime-change"):
        p = root / cold / "native-remote-connect-cold-v1.json"
        data = json.loads(p.read_bytes())
        if case == "executable-change":
            data["executable-after"]["sha256"] = "0" * 64
        else:
            data["runtime-after"][0]["sha256"] = "0" * 64
        p.write_text(json.dumps(data, indent=2) + "\n")
    elif case.startswith("positive-output-"):
        folder = root / "native-remote-cold-output-bundle-v1"
        path = folder / "receipt.json"
        data = json.loads(path.read_bytes())
        if case == "positive-output-producer":
            data["producer-receipt"]["sha256"] = "0" * 64
        else:
            log = folder / "native-output-invalid.log"
            raw = log.read_bytes()
            if case == "positive-output-buffer":
                old, new = (
                    b"buffer=0000000000000001 length=4294967295 status=00000000",
                    b"buffer=0000000000000002 length=4294967295 status=00000000",
                )
            else:
                old, new = (
                    b"length=4294967295 status=00000000",
                    b"length=4294967295 status=c0000001",
                )
            assert old in raw
            raw = raw.replace(old, new, 1)
            log.write_bytes(raw)
            data["log"].update(identity(raw))
        path.write_text(json.dumps(data, indent=2) + "\n")
    elif case != "positive":
        raise ValueError(case)


files = [
    "project-native-remote-connect-v3.py",
    "native-remote-connect-state-v2.cpp",
    "run-native-remote-connect-state-v2.py",
    "native-remote-connect-cold-v1.cpp",
    "run-native-remote-connect-cold-v1.py",
    "run-native-remote-connect-cold-output-v2.py",
]
cases = [
    "positive",
    "source-change",
    "log-change-unindexed",
    "query-value-reindexed",
    "query-coverage-reindexed",
    "callback-index-reindexed",
    "failure-marker-reindexed",
    "executable-change",
    "runtime-change",
    "positive-output-buffer",
    "positive-output-status",
    "positive-output-producer",
]
env = dict(
    os.environ,
    TMPDIR=str(D / "g"),
    TMP=str(D / "g"),
    TEMP=str(D / "g"),
    PYTHONDONTWRITEBYTECODE="1",
)
for optimization in [None, "-O", "-OO"]:
    for case in cases:
        with tempfile.TemporaryDirectory(
            prefix="remote-proof-", dir=D / "g"
        ) as directory:
            root = Path(directory)
            assert root.resolve().is_relative_to(D.resolve())
            for name in files:
                shutil.copyfile(N / name, root / name)
            for name in [
                "native-remote-connect-state-v2-bundle-v1",
                "native-remote-cold-bundle-v1",
                "native-remote-cold-output-bundle-v1",
                "remote-connect-v3-material",
            ]:
                shutil.copytree(N / name, root / name)
            mutate(root, case)
            argv = [
                sys.executable,
                "-B",
                *([optimization] if optimization else []),
                str(root / files[0]),
            ]
            child = subprocess.run(
                argv, cwd=root, env=env, capture_output=True, timeout=60
            )
            accepted = child.returncode == 0
            assert accepted == (case == "positive"), (
                optimization,
                case,
                child.stdout,
                child.stderr,
            )
            output = child.stdout + child.stderr
            row = {
                "mode": optimization or "normal",
                "case": case,
                "exit-code": child.returncode,
                "output": identity(output),
            }
            result["positive" if accepted else "negative"].append(row)
assert len(result["positive"]) == 3 and len(result["negative"]) == 33
result.update(
    state="finished",
    **{
        "exit-code": 0,
        "source": identity((N / files[0]).read_bytes()),
        "scope": "Three unmodified controls accepted;33 data/source/binary/runtime corruptions rejected across normal,-O,-OO. No production source changes; scratch directories removed.",
    }
)
record.write_text(json.dumps(result, indent=2) + "\n")
print(
    "projection controls",
    len(result["positive"]),
    "positive",
    len(result["negative"]),
    "negative",
)
