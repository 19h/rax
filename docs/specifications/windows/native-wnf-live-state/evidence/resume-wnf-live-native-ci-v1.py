from pathlib import Path
import datetime
import hashlib
import json
import subprocess
import time

S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
R = Path("C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
F = S / "wnf-live-native-candidate1-resume1.json"
assert not F.exists()
prior = json.loads((S / "wnf-live-native-candidate1.json").read_bytes())
assert prior["state"] == "failed" and prior["step"] == "ci-runner"
assert [step["exit-code"] for step in prior["steps"]] == [0, 0, 1]
assert "No module named '_overlapped'" in (S / prior["steps"][-1]["log"]).read_text()
python = Path("C:/Program Files/Python314-arm64/python.exe")
module = python.parent / "DLLs/_overlapped.pyd"


def identity(path):
    data = path.read_bytes()
    return {
        "path": str(path),
        "bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
    }


state = {
    "state": "running",
    "resumed-from": "wnf-live-native-candidate1.json",
    "source-inputs": prior["source-inputs"],
    "manifest": prior["manifest"],
    "steps": prior["steps"][:2],
    "failed-prerequisite": prior["steps"][-1],
    "python-before": identity(python),
    "overlapped-before": identity(module),
    "scope": "Use the already installed complete interpreter; preserve the incomplete embedded-runtime failure. No runtime or system configuration change.",
}
F.write_text(json.dumps(state, indent=2) + "\n")
for name, args in [
    (
        "complete-python",
        [
            str(python),
            "-I",
            "-B",
            "-c",
            "import _overlapped,sys; print(sys.version); print(_overlapped.__file__)",
        ],
    ),
    (
        "ci-runner-complete",
        [str(python), "-I", "-B", "-m", "unittest", "discover", "-s", "tools/ci"],
    ),
]:
    log = S / ("wnf-live-native-resume1-" + name + ".log")
    assert not log.exists()
    started = time.monotonic()
    with log.open("wb") as stream:
        result = subprocess.run(args, cwd=R, stdout=stream, stderr=subprocess.STDOUT)
    state["steps"].append(
        {
            "name": name,
            "argv": args,
            "exit-code": result.returncode,
            "seconds": time.monotonic() - started,
            "log": log.name,
            **{k: v for k, v in identity(log).items() if k != "path"},
        }
    )
    F.write_text(json.dumps(state, indent=2) + "\n")
    assert result.returncode == 0, name
state.update(
    state="finished",
    **{
        "exit-code": 0,
        "python-after": identity(python),
        "overlapped-after": identity(module),
        "completed-utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
)
assert state["python-before"] == state["python-after"]
assert state["overlapped-before"] == state["overlapped-after"]
F.write_text(json.dumps(state, indent=2) + "\n")
print(
    json.dumps({"state": "finished", "python": str(python), "ci-runner-exit-code": 0})
)
