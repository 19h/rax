"""Verify LPK projection integrity under normal and optimized Python."""

from pathlib import Path, PureWindowsPath
import hashlib, json, shutil, subprocess, sys, re

N = Path(__file__).resolve().parent
D = Path("/Users/int/.assist-native-display-policy-20261022")
SCRATCH = D / "g/lpk-projection-controls-v1"
RECORD = N / "lpk-projection-controls-v1.json"
assert not RECORD.exists() and not SCRATCH.exists()
assert SCRATCH.resolve().is_relative_to(D.resolve())
SCRATCH.mkdir()
BASE = SCRATCH / "base"
BASE.mkdir()
for name in (
    "native-lpk-register-v1-bundle-v1",
    "native-lpk-register-cold-v1-bundle-v1",
    "remote-connect-primary-projection-v3",
):
    shutil.copytree(N / name, BASE / name)
for name in (
    "project-native-lpk-register-v1.py",
    "native-lpk-register-v1.cpp",
    "run-native-lpk-register-v1.py",
    "native-lpk-register-cold-v1.cpp",
    "run-native-lpk-register-cold-v1.py",
    "native-lpk-kernel-imports-v1.json",
    "native-lpk-kernel-code-v1.txt",
    "lpk-register-win32k-current-v1.json",
    "lpk-registration-primary-readme-v1.md",
):
    shutil.copyfile(N / name, BASE / name)


def replace_log(case, family, relative, old, new):
    root = case / family
    path = root / relative
    raw = path.read_bytes()
    if old not in raw:
        raise ValueError("corruption target absent")
    raw = raw.replace(old, new, 1)
    path.write_bytes(raw)
    receipt = root / (
        "native-lpk-register-cold-v1.json" if "cold" in family else "receipt.json"
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
    fixed = "native-lpk-register-v1-bundle-v1"
    cold = "native-lpk-register-cold-v1-bundle-v1"
    query = "arm64/native-arm64.log"
    if name == "source-identity":
        path = case / "native-lpk-register-v1.cpp"
        path.write_bytes(path.read_bytes() + b"\n")
    elif name == "unhashed-log":
        path = case / fixed / query
        path.write_bytes(path.read_bytes() + b"altered\n")
    elif name in ("argument-coverage", "return-value", "last-error", "nt-status"):
        if name == "argument-coverage":
            old, new = b"argument=00000001", b"argument=00000002"
        elif name == "return-value":
            old, new = b"value=1", b"value=0"
        elif name == "last-error":
            old, new = b"last_error=12345678", b"last_error=12345679"
        else:
            raw = (case / fixed / query).read_bytes()
            old = re.search(rb"nt-after=([0-9A-F]{8})", raw)[0]
            new = b"nt-after=" + (
                b"00000001" if old.endswith(b"00000000") else b"00000000"
            )
        replace_log(case, fixed, query, old, new)
    elif name == "profile-missing":
        path = case / fixed / "receipt.json"
        r = json.loads(path.read_bytes())
        r["architectures"].pop()
        path.write_text(json.dumps(r))
    elif name == "cold-status":
        replace_log(
            case,
            cold,
            "native-lpk-register-cold-v1-callback-failure.log",
            b"length=0 status=c0000001",
            b"length=0 status=00000000",
        )
    elif name == "cold-marker":
        replace_log(
            case,
            cold,
            "native-lpk-register-cold-v1-normal.log",
            b"cold-query-return value=1 last_error=12345678 win32-thread-present=1",
            b"cold-query-return value=1 last_error=12345678 win32-thread-present=0",
        )
    elif name == "cold-invalid-output":
        replace_log(
            case,
            cold,
            "native-lpk-register-cold-v1-output-invalid.log",
            b"length=4294967295 status=00000000",
            b"length=4294967294 status=00000000",
        )
    elif name in ("kernel-input", "kernel-current"):
        path = case / (
            "native-lpk-kernel-imports-v1.json"
            if name == "kernel-input"
            else "lpk-register-win32k-current-v1.json"
        )
        r = json.loads(path.read_bytes())
        target = r["input"] if name == "kernel-input" else r
        target["sha256"] = "0" * 64
        path.write_text(json.dumps(r))
    else:
        raise ValueError(name)


names = (
    "source-identity",
    "unhashed-log",
    "argument-coverage",
    "return-value",
    "last-error",
    "nt-status",
    "profile-missing",
    "cold-status",
    "cold-marker",
    "cold-invalid-output",
    "kernel-input",
    "kernel-current",
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
                str(case / "project-native-lpk-register-v1.py"),
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
                "negative": 36,
                "rows": rows,
                "scope": "Projection integrity only; no independent kernel behavior proof.",
                "generator-sha256": hashlib.sha256(
                    (N / "project-native-lpk-register-v1.py").read_bytes()
                ).hexdigest(),
            },
            indent=2,
        )
        + "\n"
    )
    print("accepted3 baseline projections; rejected36 altered LPK inputs")
finally:
    assert SCRATCH.resolve().is_relative_to(D.resolve())
    shutil.rmtree(SCRATCH)
