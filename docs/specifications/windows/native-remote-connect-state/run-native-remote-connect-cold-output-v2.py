"""Reuse the verified native cold-control binary for positive invalid-output coverage."""

from pathlib import Path
import hashlib
import json
import os
import subprocess
import traceback

R = Path("C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
OLD = R / "target/native-remote-connect-cold-v1"
OUT = R / "target/native-remote-connect-cold-output-v2"
assert not OUT.exists()
OUT.mkdir()
RECORD = OUT / "receipt.json"
state = {"state": "starting", "pid": os.getpid()}


def identity(path):
    raw = path.read_bytes()
    return {
        "file": str(path),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
    }


def save():
    RECORD.write_text(json.dumps(state, indent=2) + "\n")


save()
try:
    prior = json.loads((OLD / "native-remote-connect-cold-v1.json").read_bytes())
    assert prior["state"] == "finished" and prior["exit-code"] == 0
    source = Path(prior["source-before"]["file"])
    executable = Path(prior["executable-before"]["file"])
    assert identity(source) == prior["source-before"] == prior["source-after"]
    assert (
        identity(executable) == prior["executable-before"] == prior["executable-after"]
    )
    state["producer-receipt"] = identity(OLD / "native-remote-connect-cold-v1.json")
    state["source-before"] = identity(source)
    state["executable-before"] = identity(executable)
    modules = [Path(row["file"]) for row in prior["runtime-before"]]
    state["runtime-before"] = [identity(path) for path in modules]
    assert state["runtime-before"] == prior["runtime-after"]
    log = OUT / "native-output-invalid.log"
    with log.open("wb") as output:
        child = subprocess.Popen(
            [str(executable), "output-invalid"],
            cwd=OUT,
            stdin=subprocess.DEVNULL,
            stdout=output,
            stderr=output,
        )
        state.update(state="running", child=child.pid)
        save()
        code = child.wait(timeout=120)
    state["native-exit-code"] = code
    state["log"] = identity(log)
    state["source-after"] = identity(source)
    state["executable-after"] = identity(executable)
    state["runtime-after"] = [identity(path) for path in modules]
    for key in ("source", "executable", "runtime"):
        assert state[key + "-before"] == state[key + "-after"], key
    assert code == 0, code
    state.update(state="finished", **{"exit-code": 0})
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    save()
print(json.dumps(state))
raise SystemExit(state["exit-code"])
