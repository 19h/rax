"""Falsify altered observations and provenance, including optimized Python."""

from pathlib import Path, PureWindowsPath
import hashlib
import json
import re
import shutil
import subprocess
import sys

N = Path(__file__).resolve().parent
D = Path("/Users/int/.assist-native-display-policy-20261022")
SCRATCH = D / "g/process-ui-context-projection-controls-v1"
RECORD = N / "process-ui-context-projection-controls-v1.json"
assert not SCRATCH.exists() and not RECORD.exists()
assert SCRATCH.resolve().is_relative_to(D.resolve())
SCRATCH.mkdir()
BASE = SCRATCH / "base"
BASE.mkdir()
labels = (
    "native-process-ui-context-v2",
    "native-process-ui-context-cold-v1",
    "native-process-ui-context-cold-v2",
)
for label in labels:
    shutil.copytree(N / (label + "-bundle-v1"), BASE / (label + "-bundle-v1"))
    for name in (label + ".cpp", "run-" + label + ".py"):
        shutil.copyfile(N / name, BASE / name)
shutil.copytree(
    N / "remote-connect-primary-projection-v3",
    BASE / "remote-connect-primary-projection-v3",
)
for name in (
    "project-native-process-ui-context-v3.py",
    "native-process-ui-context-kernel-code-v1.txt",
    "lpk-register-win32k-current-v1.json",
    "process-ui-context-primary-readme-v1.md",
):
    shutil.copyfile(N / name, BASE / name)


def rewrite_log(case, label, relative, transform):
    root = case / (label + "-bundle-v1")
    path = root / relative
    before = path.read_bytes()
    after = transform(before)
    if before == after:
        raise ValueError("absent corruption target")
    path.write_bytes(after)
    filename = label + ".json" if "cold" in label else "receipt.json"
    receipt = root / filename
    record = json.loads(receipt.read_bytes())
    source = record["source-before"]
    origin = PureWindowsPath(source.get("path", source.get("file"))).parent
    found = False
    for row in record["steps"]:
        identity = row["log"]
        name = (
            PureWindowsPath(identity.get("path", identity.get("file")))
            .relative_to(origin)
            .as_posix()
        )
        if name == relative:
            identity.update(bytes=len(after), sha256=hashlib.sha256(after).hexdigest())
            found = True
    if not found:
        raise ValueError("receipt log absent")
    receipt.write_text(json.dumps(record, indent=2) + "\n")


def corrupt(case, name):
    fixed, cold = labels[0], labels[1]
    relative = "arm64/native-arm64.log"
    if name == "source-identity":
        path = case / (fixed + ".cpp")
        path.write_bytes(path.read_bytes() + b"\n")
    elif name == "unhashed-log":
        path = case / (fixed + "-bundle-v1") / relative
        path.write_bytes(path.read_bytes() + b"altered\n")
    elif name in ("profile-missing", "executable-identity"):
        path = case / (fixed + "-bundle-v1/receipt.json")
        record = json.loads(path.read_bytes())
        if name == "profile-missing":
            record["architectures"].pop()
        else:
            record["architectures"][0]["executable-after"]["sha256"] = "0" * 64
        path.write_text(json.dumps(record))
    elif name in (
        "coverage",
        "return",
        "error",
        "extent",
        "output",
        "status",
        "guard-priority",
        "unreadable-disclosure",
    ):
        if name == "coverage":
            old, new = (
                b"role=current buffer=stack offset=0 sample=0",
                b"role=current buffer=stack offset=1 sample=0",
            )
        elif name == "return":
            old, new = b"result=1", b"result=0"
        elif name == "error":
            old, new = b"error=12345678 nt-before", b"error=12345679 nt-before"
        elif name == "extent":
            old, new = b"stack-changed=8", b"stack-changed=4"
        elif name == "output":
            old, new = b"data=0000000000000000", b"data=0000000100000000"
        elif name == "status":
            old, new = b"nt-after=C0000005", b"nt-after=00000000"
        elif name == "unreadable-disclosure":
            old, new = (
                b"readable=0 data= stack-changed",
                b"readable=0 data=FFFFFFFFFFFFFFFF stack-changed",
            )
        else:
            raw = (case / (fixed + "-bundle-v1") / relative).read_bytes()
            old = re.search(
                rb"QUERY phase=main role=current buffer=guard [^\r\n]+", raw
            )[0]
            new = old.replace(b"protect=00000004", b"protect=00000104")
        rewrite_log(case, fixed, relative, lambda raw: raw.replace(old, new, 1))
    elif name in ("cold-status", "cold-marker", "cold-callback"):
        mode = "callback-failure" if name == "cold-status" else "normal"
        if name == "cold-status":
            old, new = b"length=0 status=c0000001", b"length=0 status=00000000"
        elif name == "cold-marker":
            old, new = (
                b"value=1 last_error=12345678 win32-thread-present=1",
                b"value=1 last_error=12345678 win32-thread-present=0",
            )
        else:
            old, new = b"index=84 length=0", b"index=85 length=0"
        rewrite_log(
            case, cold, cold + "-" + mode + ".log", lambda raw: raw.replace(old, new, 1)
        )
    elif name == "rejected-mode":
        path = case / (cold + "-bundle-v1") / (cold + ".json")
        record = json.loads(path.read_bytes())
        next(row for row in record["native-outcomes"] if row["mode"] == "query-null")[
            "exit-code"
        ] = 0
        path.write_text(json.dumps(record))
    elif name == "kernel-input":
        path = case / "lpk-register-win32k-current-v1.json"
        record = json.loads(path.read_bytes())
        record["sha256"] = "0" * 64
        path.write_text(json.dumps(record))
    else:
        raise ValueError(name)


names = (
    "source-identity",
    "unhashed-log",
    "profile-missing",
    "executable-identity",
    "coverage",
    "return",
    "error",
    "extent",
    "output",
    "status",
    "guard-priority",
    "unreadable-disclosure",
    "cold-status",
    "cold-marker",
    "cold-callback",
    "rejected-mode",
    "kernel-input",
)
rows = []
try:
    for optimizer in ("", "-O", "-OO"):
        for name in ("positive", *names):
            case = SCRATCH / ((optimizer or "normal").replace("-", "") + "-" + name)
            shutil.copytree(BASE, case)
            if name != "positive":
                corrupt(case, name)
            argv = [
                sys.executable,
                "-I",
                "-B",
                *([optimizer] if optimizer else []),
                str(case / "project-native-process-ui-context-v3.py"),
            ]
            result = subprocess.run(argv, capture_output=True)
            if (result.returncode == 0) != (name == "positive"):
                raise ValueError(
                    (optimizer, name, result.returncode, result.stderr.decode())
                )
            rows.append(
                {
                    "optimizer": optimizer or "normal",
                    "case": name,
                    "accepted": result.returncode == 0,
                    "exit-code": result.returncode,
                    "stderr-sha256": hashlib.sha256(result.stderr).hexdigest(),
                }
            )
            shutil.rmtree(case)
    RECORD.write_text(
        json.dumps(
            {
                "state": "finished",
                "positive": 3,
                "negative": len(names) * 3,
                "rows": rows,
                "scope": "Projection integrity only; not independent kernel proof.",
                "generator-sha256": hashlib.sha256(
                    (N / "project-native-process-ui-context-v3.py").read_bytes()
                ).hexdigest(),
            },
            indent=2,
        )
        + "\n"
    )
    print("accepted3 baselines; rejected", len(names) * 3, "altered inputs")
finally:
    assert SCRATCH.resolve().is_relative_to(D.resolve())
    shutil.rmtree(SCRATCH)
