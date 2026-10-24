"""Build and execute three owned MUI query observers on the native ARM64 host."""

from pathlib import Path
import base64
import hashlib
import json
import os
import struct
import subprocess
import sys
import traceback

R = Path("C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
OUT = R / "target/native-mui-settings-v2"
assert not OUT.exists()
OUT.mkdir()
assert OUT.resolve().is_relative_to(R.resolve())
source = OUT / "native-mui-settings-v2.cpp"
source.write_bytes(base64.b64decode(sys.stdin.readline().strip(), validate=True))
record = OUT / "receipt.json"
state = {"state": "starting", "pid": os.getpid(), "steps": [], "architectures": []}


def identity(path):
    raw = path.read_bytes()
    return {
        "path": str(path),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
    }


def save():
    record.write_text(json.dumps(state, indent=2) + "\n")


def run(name, argv, directory, env):
    log = directory / (name + ".log")
    with log.open("wb") as output:
        child = subprocess.Popen(
            [str(x) for x in argv], cwd=directory, env=env, stdout=output, stderr=output
        )
        state.update(state="running", step=name, child=child.pid)
        save()
        code = child.wait()
    state["steps"].append(
        {
            "name": name,
            "argv": [str(x) for x in argv],
            "exit-code": code,
            "log": identity(log),
        }
    )
    save()
    assert code == 0, (name, code)


save()
try:
    state["source-before"] = identity(source)
    environment = (
        R
        / "target/win32u-validation-recovery-v2/transfer/mutant-native-compiler-env-epoch4.json"
    )
    assert (
        identity(environment)["sha256"]
        == "e6ca43bcdcbd3067b5de05d051259d4d3abf0bfa5283a98daac14fd508a05fad"
    )
    base = json.loads(environment.read_text(encoding="utf-8-sig"))
    modules = [
        Path("C:/Windows") / part / "ntdll.dll" for part in ["System32", "SysWOW64"]
    ]
    state["native-modules-before"] = [identity(path) for path in modules]
    for profile, arch, machine in [
        ("arm64", "arm64", 0xAA64),
        ("x86", "x86", 0x14C),
        ("x64", "x64", 0x8664),
        ("x86-laa", "x86", 0x14C),
    ]:
        directory = OUT / profile
        directory.mkdir()
        (directory / "tmp").mkdir()
        env = dict(os.environ)
        for key in ["CL", "_CL_", "LINK", "_LINK_", "CCACHE_DIR", "SCCACHE_DIR"]:
            env.pop(key, None)
        env.update(base)
        for key in ["LIB", "LIBPATH"]:
            env[key] = ";".join(
                (
                    str(Path(value).with_name(arch))
                    if value and Path(value).name.lower() == "arm64"
                    else value
                )
                for value in base[key].split(";")
            )
        compiler = Path(base["VCToolsInstallDir"]) / "bin/Hostarm64" / arch / "cl.exe"
        env.update(
            PATH=str(compiler.parent) + ";" + base["PATH"],
            TMP=str(directory / "tmp"),
            TEMP=str(directory / "tmp"),
            TMPDIR=str(directory / "tmp"),
            CCACHE_DISABLE="1",
            SCCACHE_DISABLE="1",
            VSCMD_ARG_TGT_ARCH=arch,
        )
        exe = directory / "probe.exe"
        row = {"profile": profile, "architecture": arch, "compiler": identity(compiler)}
        state["architectures"].append(row)
        run(
            "build-" + arch,
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
                source,
                "/link",
                "advapi32.lib",
                (
                    "/LARGEADDRESSAWARE"
                    if profile == "x86-laa"
                    else (
                        "/LARGEADDRESSAWARE:NO"
                        if profile == "x86"
                        else "/LARGEADDRESSAWARE"
                    )
                ),
                "/SUBSYSTEM:CONSOLE,10.0",
                "/OUT:" + str(exe),
            ],
            directory,
            env,
        )
        raw = exe.read_bytes()
        offset = struct.unpack_from("<I", raw, 0x3C)[0]
        row["machine"] = struct.unpack_from("<H", raw, offset + 4)[0]
        assert row["machine"] == machine
        row["executable-before"] = identity(exe)
        run("native-" + profile, [exe, profile], directory, env)
        row["executable-after"] = identity(exe)
        assert row["executable-before"] == row["executable-after"]
        save()
    state["source-after"] = identity(source)
    state["native-modules-after"] = [identity(path) for path in modules]
    assert state["source-before"] == state["source-after"]
    assert state["native-modules-before"] == state["native-modules-after"]
    state.update(state="finished", **{"exit-code": 0})
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    save()
print(json.dumps(state))
raise SystemExit(state["exit-code"])
