from pathlib import Path
import datetime
import hashlib
import json
import subprocess
import time
import traceback

S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
B = Path("C:/Windows/Temp/assist-native-rax-20261010")
R = B / "src-display-candidate1"
F = S / "native-read-repeat-v3.json"
assert not F.exists()
state = {
    "state": "starting",
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "steps": [],
}


def identity(path):
    path = Path(path)
    raw = path.read_bytes()
    return {
        "path": str(path),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
    }


def pair(row):
    return row["bytes"], row["sha256"]


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


save()
try:
    modules = [
        Path("C:/Windows") / part / dll
        for dll in ["ntdll.dll", "KernelBase.dll", "kernel32.dll", "msvcrt.dll"]
        for part in ["System32", "SysWOW64"]
    ]
    state["runtime-modules-before"] = [identity(p) for p in modules]
    for version in range(6, 8):
        original_path = (
            S / f"native-read-virtual-memory-v{version}-four-views-launch.json"
        )
        original = json.loads(original_path.read_bytes())
        assert original["state"] == "finished"
        assert original["runtime-modules-before"] == state["runtime-modules-before"]
        assert identity(original["source-before"]["path"]) == original["source-before"]
        for row in original["outcomes"]:
            profile = row["profile"]
            exe = Path(row["executable-before"]["path"])
            assert identity(exe) == row["executable-before"]
            log = S / f"native-read-repeat-v3-producer{version}-{profile}.log"
            assert not log.exists()
            start = time.monotonic()
            with log.open("wb") as output:
                result = subprocess.run(
                    [str(exe), profile],
                    cwd=exe.parent,
                    stdout=output,
                    stderr=subprocess.STDOUT,
                    timeout=120,
                )
            assert identity(exe) == row["executable-before"]
            raw = log.read_bytes()
            expected = [
                json.loads(line)
                for line in (S / row["log"]).read_text().splitlines()
                if line.startswith("{")
            ]
            observed = [
                json.loads(line)
                for line in raw.decode().splitlines()
                if line.startswith("{")
            ]
            # Retain exact residency observations. Host prefetch breadth is a
            # diagnostic, not normalized byte/protection/status evidence.
            project = lambda rows: [
                {
                    key: value
                    for key, value in row.items()
                    if key not in ["resident-before", "resident-after"]
                }
                for row in rows
            ]
            stable = project(expected) == project(observed)
            state["steps"].append(
                {
                    "version": version,
                    "profile": profile,
                    "exit-code": result.returncode,
                    "seconds": time.monotonic() - start,
                    "log": log.name,
                    "bytes": len(raw),
                    "sha256": hashlib.sha256(raw).hexdigest(),
                    "calls": len(observed),
                    "complete-nonresidency-row-match": stable,
                    "original-launch": identity(original_path),
                    "executable-before": row["executable-before"],
                    "executable-after": identity(exe),
                    "source": identity(original["source-before"]["path"]),
                }
            )
            save()
            assert result.returncode == 0 and stable
    state["runtime-modules-after"] = [identity(p) for p in modules]
    assert state["runtime-modules-before"] == state["runtime-modules-after"]
    state.update(state="finished", **{"exit-code": 0})
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    save()
raise SystemExit(state["exit-code"])
