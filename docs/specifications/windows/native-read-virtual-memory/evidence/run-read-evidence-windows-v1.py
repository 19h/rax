"""Run the final evidence verifier against the actual native Windows source."""

from pathlib import Path
import datetime
import hashlib
import json
import os
import platform
import subprocess
import sys
import traceback

S = Path(r"C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154")
R = Path(r"C:\Windows\Temp\assist-native-rax-20261010\src-display-candidate1")
D = Path(
    r"C:\Mac\Home\.assist-native-display-policy-20261022\docs\specifications\windows\native-read-virtual-memory"
)
F = S / "read-memory-evidence-windows-v1.json"
assert os.name == "nt" and not F.exists()


def identity(path):
    raw = path.read_bytes()
    return {
        "path": str(path),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
    }


state = {
    "state": "preparing",
    "os": os.name,
    "machine": platform.machine(),
    "source-root": str(R),
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


save()
try:
    for field, name in [("checker", "check_evidence.py"), ("proof", "proof.json")]:
        state[field + "-before"] = identity(D / name)
    log = S / "read-memory-evidence-windows-v1.log"
    assert not log.exists()
    argv = [
        sys.executable,
        "-B",
        str(D / "check_evidence.py"),
        "--evidence-dir",
        str(D),
        "--source-root",
        str(R),
    ]
    state.update(state="running", argv=argv)
    save()
    with log.open("wb") as stream:
        child = subprocess.run(argv, cwd=R, stdout=stream, stderr=subprocess.STDOUT)
    state["log"] = identity(log)
    state["verifier-exit-code"] = child.returncode
    for field, name in [("checker", "check_evidence.py"), ("proof", "proof.json")]:
        state[field + "-after"] = identity(D / name)
        assert state[field + "-before"] == state[field + "-after"], field
    assert child.returncode == 0
    state.update(state="finished", **{"exit-code": 0})
except BaseException:
    state.update(state="failed", error=traceback.format_exc(), **{"exit-code": 1})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
raise SystemExit(state["exit-code"])
