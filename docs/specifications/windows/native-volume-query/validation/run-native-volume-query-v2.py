"""Independent native ARM64 CurrentEL read with a real SEH filter."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import traceback

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
R = B / "native-volume-query-v2"
F = S / "native-volume-query-v2-launch.json"
assert not R.exists() and not F.exists()
state = {"state": "preparing", "steps": []}


def identity(path):
    raw = path.read_bytes()
    return {
        "path": str(path),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
    }


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def run(name, argv):
    log = S / ("native-volume-query-v2-" + name + ".log")
    with log.open("wb") as output:
        child = subprocess.Popen(
            [str(x) for x in argv],
            cwd=R,
            env=env,
            stdout=output,
            stderr=subprocess.STDOUT,
        )
        state.update(state="running", step=name, child=child.pid)
        save()
        code = child.wait(timeout=180)
    state["steps"].append(
        {
            "name": name,
            "argv": [str(x) for x in argv],
            "exit-code": code,
            **identity(log),
        }
    )
    save()
    assert code == 0, name


save()
try:
    R.mkdir()
    source = R / "native-volume-query-v2.cpp"
    shutil.copy2(S / source.name, source)
    state["source-before"] = identity(source)
    assert (
        state["source-before"]["sha256"]
        == "bbad6510602232baad4a5acea706986ee5f1401af02aa82467198b55189d441c"
    )
    env = dict(os.environ)
    env.update(
        json.loads(
            (S / "mutant-native-compiler-env-epoch4.json").read_text(
                encoding="utf-8-sig"
            )
        )
    )
    compiler = Path(env["VCToolsInstallDir"]) / "bin/Hostarm64/arm64/cl.exe"
    env.update(PATH=str(compiler.parent) + ";" + env["PATH"], TMP=str(R), TEMP=str(R))
    for name in ["CL", "_CL_", "LINK", "_LINK_"]:
        env.pop(name, None)
    modules = [
        Path("C:/Windows") / kind / name
        for name in ["ntdll.dll", "kernel32.dll", "KernelBase.dll", "msvcrt.dll"]
        for kind in ["System32", "SysWOW64"]
    ]
    state["modules-before"] = [identity(p) for p in modules]
    executable = R / "probe.exe"
    run(
        "build",
        [
            compiler,
            "/nologo",
            "/std:c++20",
            "/EHsc",
            "/MT",
            "/Od",
            "/we4473",
            "/we4474",
            "/we4477",
            source,
            "/link",
            "/SUBSYSTEM:CONSOLE,10.0",
            "/OUT:" + str(executable),
        ],
    )
    inspect = [
        B / "python/python.exe",
        "-B",
        S / "inspect-system-time-v2-pe.py",
        executable,
        "arm64",
    ]
    run("before", inspect)
    state["executable-before"] = identity(executable)
    run("native", [executable])
    state["executable-after"] = identity(executable)
    run("after", inspect)
    state["source-after"] = identity(source)
    state["modules-after"] = [identity(p) for p in modules]
    for key in ["source", "executable", "modules"]:
        assert state[key + "-before"] == state[key + "-after"]
    state.update(state="finished", **{"exit-code": 0})
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    save()
print(json.dumps(state), flush=True)
raise SystemExit(state["exit-code"])
