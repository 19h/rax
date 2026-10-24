"""Build and run a private-process ARM64 first-entry callback observer."""

from pathlib import Path
import base64
import datetime
import hashlib
import json
import os
import struct
import subprocess
import sys
import traceback

RAX = Path("C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
ROOT = RAX / "target/native-remote-connect-cold-v1"
ENVIRONMENT = (
    RAX
    / "target/win32u-validation-recovery-v2/transfer/mutant-native-compiler-env-epoch4.json"
)
assert not ROOT.exists()
assert ROOT.resolve().is_relative_to(RAX.resolve())
ROOT.mkdir()
(ROOT / "tmp").mkdir()
SOURCE = ROOT / "native-remote-connect-cold-v1.cpp"
SOURCE.write_bytes(base64.b64decode(sys.stdin.readline().strip(), validate=True))
RECORD = ROOT / "native-remote-connect-cold-v1.json"
result = {
    "state": "starting",
    "pid": os.getpid(),
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "scope": "Observe normal main/worker conversion and controlled zero-input callback returns in separate native ARM64 processes. Nonzero controlled native exits remain explicit in native-outcomes; finished means recording completed. No on-disk DLL changes or guest import of host pointers.",
    "steps": [],
}


def identity(path):
    raw = path.read_bytes()
    return {
        "file": str(path),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
    }


def save():
    RECORD.write_text(json.dumps(result, indent=2) + "\n")


def run(name, argv, cwd, environment):
    log = ROOT / ("native-remote-connect-cold-v1-" + name + ".log")
    with log.open("wb") as output:
        child = subprocess.Popen(
            [str(arg) for arg in argv],
            cwd=cwd,
            env=environment,
            stdout=output,
            stderr=subprocess.STDOUT,
        )
        result.update(state="running", step=name, child=child.pid)
        save()
        code = child.wait(timeout=600)
    result["steps"].append(
        {
            "name": name,
            "argv": [str(x) for x in argv],
            "exit-code": code,
            "log": identity(log),
        }
    )
    save()
    return code


save()
try:
    result["source-before"] = identity(SOURCE)
    result["environment"] = identity(ENVIRONMENT)
    assert result["environment"]["sha256"] == (
        "e6ca43bcdcbd3067b5de05d051259d4d3abf0bfa5283a98daac14fd508a05fad"
    )
    base = json.loads(ENVIRONMENT.read_text(encoding="utf-8-sig"))
    runtime = [
        Path("C:/Windows/System32") / name
        for name in [
            "ntdll.dll",
            "kernel32.dll",
            "KernelBase.dll",
            "user32.dll",
            "win32u.dll",
            "gdi32full.dll",
            "gdi32.dll",
        ]
    ]
    result["runtime-before"] = [identity(path) for path in runtime]
    environment = dict(os.environ)
    for name in ["CL", "_CL_", "LINK", "_LINK_"]:
        environment.pop(name, None)
    environment.update(base)
    compiler = Path(base["VCToolsInstallDir"]) / "bin/Hostarm64/arm64/cl.exe"
    result["compiler"] = identity(compiler)
    environment.update(
        PATH=str(compiler.parent) + ";" + base["PATH"],
        TMP=str(ROOT / "tmp"),
        TEMP=str(ROOT / "tmp"),
        TMPDIR=str(ROOT / "tmp"),
        CCACHE_DISABLE="1",
        SCCACHE_DISABLE="1",
        VSCMD_ARG_TGT_ARCH="arm64",
        PYTHONDONTWRITEBYTECODE="1",
    )
    exe = ROOT / "probe.exe"
    code = run(
        "build",
        [
            compiler,
            "/nologo",
            "/std:c++20",
            "/EHsc",
            "/MT",
            "/GS",
            "/Od",
            "/we4473",
            "/we4474",
            "/we4477",
            SOURCE,
            "/link",
            "/SUBSYSTEM:CONSOLE,10.0",
            "/OUT:" + str(exe),
        ],
        ROOT,
        environment,
    )
    assert code == 0, "native build"
    raw = exe.read_bytes()
    pe = struct.unpack_from("<I", raw, 0x3C)[0]
    assert raw[:2] == b"MZ" and raw[pe : pe + 4] == b"PE\0\0"
    result["machine"] = struct.unpack_from("<H", raw, pe + 4)[0]
    assert result["machine"] == 0xAA64
    result["executable-before"] = identity(exe)
    result["native-outcomes"] = []
    for mode in [
        "normal",
        "callback-success",
        "callback-failure",
        "output-warning",
    ]:
        before = identity(exe)
        native_code = run(mode, [exe, mode], ROOT, environment)
        after = identity(exe)
        result["native-outcomes"].append(
            {"mode": mode, "exit-code": native_code, "before": before, "after": after}
        )
        assert before == after
        if mode == "normal":
            assert native_code == 0, "normal observation"
    code = 0
    result["executable-after"] = identity(exe)
    result["source-after"] = identity(SOURCE)
    result["runtime-after"] = [identity(path) for path in runtime]
    assert result["executable-before"] == result["executable-after"]
    assert result["source-before"] == result["source-after"]
    assert result["runtime-before"] == result["runtime-after"]
    assert code == 0, "native observation"
    result.update(state="finished", **{"exit-code": 0})
except BaseException:
    result.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    result["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps(result), flush=True)
raise SystemExit(result["exit-code"])
