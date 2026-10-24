"""Exercise projection invariants with rehashed corruption under Python optimizers."""

from pathlib import Path, PureWindowsPath
import hashlib
import json
import shutil
import subprocess
import sys

N = Path(__file__).resolve().parent
D = Path("/Users/int/.assist-native-display-policy-20261022")
SCRATCH = D / "g/thread-desktop-projection-controls-v1"
RECORD = N / "thread-desktop-projection-controls-v1.json"
assert not RECORD.exists() and not SCRATCH.exists()
assert SCRATCH.resolve().is_relative_to(D.resolve())
SCRATCH.mkdir()
BASE = SCRATCH / "base"
BASE.mkdir()
for name in (
    "native-thread-desktop-v3-bundle-v1",
    "native-thread-desktop-v4-bundle-v1",
    "native-thread-desktop-cold-v1-bundle-v1",
):
    shutil.copytree(N / name, BASE / name)
for version in ("v3", "v4", "cold-v1"):
    name = "native-thread-desktop-" + version
    for path in (name + ".cpp", "run-" + name + ".py"):
        shutil.copyfile(N / path, BASE / path)
shutil.copyfile(
    N / "project-native-thread-desktop-v2.py",
    BASE / "project-native-thread-desktop-v2.py",
)


def replace_log(case, family, relative, old, new):
    root = case / family
    path = root / relative
    raw = path.read_bytes()
    if old not in raw:
        raise ValueError("corruption target absent")
    raw = raw.replace(old, new, 1)
    path.write_bytes(raw)
    receipt = root / (
        "native-thread-desktop-cold-v1.json" if "cold" in family else "receipt.json"
    )
    record = json.loads(receipt.read_bytes())
    identity = record["source-before"]
    origin = PureWindowsPath(identity.get("path", identity.get("file"))).parent
    found = False
    for row in record["steps"]:
        log = row["log"]
        name = (
            PureWindowsPath(log.get("path", log.get("file")))
            .relative_to(origin)
            .as_posix()
        )
        if name == relative:
            log.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
            found = True
    if not found:
        raise ValueError("receipt log absent")
    receipt.write_text(json.dumps(record, indent=2) + "\n")


def corrupt(case, name):
    family = "native-thread-desktop-v3-bundle-v1"
    prefix = "arm64/native-arm64-"
    if name == "source-identity":
        path = case / "native-thread-desktop-v3.cpp"
        path.write_bytes(path.read_bytes() + b"\n")
    elif name == "unhashed-log":
        path = case / family / (prefix + "query.log")
        path.write_bytes(path.read_bytes() + b"altered\n")
    elif name == "query-error":
        replace_log(
            case,
            family,
            prefix + "query.log",
            b"error=12345678 nt-before=",
            b"error=12345679 nt-before=",
        )
    elif name == "query-count":
        replace_log(
            case,
            family,
            prefix + "query.log",
            b"QUERY phase=main",
            b"REMOVED phase=main",
        )
    elif name == "metadata-grant":
        replace_log(
            case, family, prefix + "query.log", b"access=000F00CF", b"access=000F00CE"
        )
    elif name == "handle-flags":
        replace_log(
            case, family, prefix + "query.log", b"flags=00000000", b"flags=00000002"
        )
    elif name == "assigned-close":
        replace_log(
            case,
            family,
            prefix + "nt-close.log",
            b"status=C0000235",
            b"status=C0000008",
        )
    elif name == "assigned-final-identity":
        replace_log(
            case,
            family,
            prefix + "dup-close-source.log",
            b"NAME label=current-final ok=1",
            b"NAME label=current-final ok=0",
        )
    elif name == "worker-conversion":
        replace_log(
            case,
            "native-thread-desktop-v4-bundle-v1",
            prefix + "query.log",
            b"gui-before=0 gui-after=1",
            b"gui-before=0 gui-after=0",
        )
    elif name == "cold-failure-status":
        replace_log(
            case,
            "native-thread-desktop-cold-v1-bundle-v1",
            "native-thread-desktop-cold-v1-callback-failure.log",
            b"length=0 status=c0000001",
            b"length=0 status=00000000",
        )
    elif name == "cold-output-size":
        replace_log(
            case,
            "native-thread-desktop-cold-v1-bundle-v1",
            "native-thread-desktop-cold-v1-output-invalid.log",
            b"length=4294967295 status=00000000",
            b"length=4294967294 status=00000000",
        )
    elif name == "cold-error":
        replace_log(
            case,
            "native-thread-desktop-cold-v1-bundle-v1",
            "native-thread-desktop-cold-v1-normal.log",
            b"last_error=12345678",
            b"last_error=12345679",
        )
    else:
        raise ValueError(name)


names = (
    "source-identity",
    "unhashed-log",
    "query-error",
    "query-count",
    "metadata-grant",
    "handle-flags",
    "assigned-close",
    "assigned-final-identity",
    "worker-conversion",
    "cold-failure-status",
    "cold-output-size",
    "cold-error",
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
                str(case / "project-native-thread-desktop-v2.py"),
            ]
            result = subprocess.run(argv, capture_output=True)
            expected = name == "positive"
            if (result.returncode == 0) != expected:
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
                "negative": 36,
                "rows": rows,
                "scope": "Integrity and semantic projection controls, not independent kernel-behavior proof.",
            },
            indent=2,
        )
        + "\n"
    )
    print("accepted3 baseline projections; rejected36 rehashed corruptions")
finally:
    assert SCRATCH.resolve().is_relative_to(D.resolve())
    shutil.rmtree(SCRATCH)
