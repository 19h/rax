"""Verify one frozen proof and retain stable before/after identities."""

from pathlib import Path
import hashlib
import json
import os
import platform
import subprocess
import sys

Q = Path(__file__).resolve().parent
R = Path("/Users/int/.assist-native-display-policy-20261022")
D = R / "docs/specifications/windows/native-gre-initialize"
HOST = sys.argv[1]
assert HOST in ["macos", "linux"]
F = Q / f"gre-initialize-evidence-{HOST}-final-v2.json"
L = Q / f"gre-initialize-evidence-{HOST}-final-v2.log"
assert not F.exists() and not L.exists()


def identity(path):
    b = path.read_bytes()
    return {"file": path.name, "bytes": len(b), "sha256": hashlib.sha256(b).hexdigest()}


state = {
    "state": "preparing",
    "host": HOST,
    "launcher-os": os.name,
    "launcher-machine": platform.machine(),
}
for field, name in [("checker", "check_evidence.py"), ("proof", "proof.json")]:
    state[field + "-before"] = identity(D / name)
argv = [sys.executable, "-B", str(D / "check_evidence.py")]
if HOST == "linux":
    argv = [
        "docker",
        "run",
        "--rm",
        "--platform",
        "linux/amd64",
        "-v",
        str(R) + ":/work/vendor/rax:ro",
        "-w",
        "/work/vendor/rax",
        "assist-rax-validation-rustfmt:1.95-bullseye-20261015",
        "python3",
        "-B",
        "docs/specifications/windows/native-gre-initialize/check_evidence.py",
    ]
    state["execution-boundary"] = (
        "Linux x64 container with translation on ARM64; not a physical x64 instruction oracle"
    )
state.update(state="running", argv=argv)
F.write_text(json.dumps(state, indent=2) + "\n")
with L.open("wb") as stream:
    child = subprocess.run(argv, cwd=R, stdout=stream, stderr=subprocess.STDOUT)
state.update(**{"exit-code": child.returncode, "log": identity(L)})
for field, name in [("checker", "check_evidence.py"), ("proof", "proof.json")]:
    state[field + "-after"] = identity(D / name)
    assert state[field + "-before"] == state[field + "-after"], field
state["state"] = "finished" if child.returncode == 0 else "failed"
F.write_text(json.dumps(state, indent=2) + "\n")
print(HOST, child.returncode, flush=True)
raise SystemExit(child.returncode)
