from pathlib import Path
import datetime
import hashlib
import json
import subprocess
import tarfile
import time
import traceback

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
R = B / "fo-fa1"
F = S / "file-open-public-native-final-v1.json"
assert not R.exists() and not F.exists()
state = {"state": "preparing", "pid": __import__("os").getpid(), "steps": []}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def identity(path):
    raw = path.read_bytes()
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


save()
try:
    archive = S / "file-open-public-final-v1.tar.gz"
    meta = json.loads((S / "file-open-public-final-v1-archive.json").read_text())
    assert identity(archive) == {k: meta[k] for k in ["bytes", "sha256"]}
    R.mkdir()
    with tarfile.open(archive, "r:gz") as tar:
        assert all(
            not Path(m.name).is_absolute()
            and ".." not in Path(m.name).parts
            and (m.isfile() or m.isdir())
            for m in tar.getmembers()
        )
        tar.extractall(R, filter="data")
    D = R / "native-file-open"
    source = B / "src-file-open-epoch2"
    python = str(B / "python/python.exe")
    commands = [
        (
            "final-audit",
            [
                python,
                "-B",
                str(D / "check_validation.py"),
                "--source-root",
                str(source),
                "--output",
                str(S / "file-open-public-audit-windows-final.json"),
            ],
        ),
    ]
    for name, argv in commands:
        log = S / ("file-open-public-native-final-v1-" + name + ".log")
        start = time.monotonic()
        with log.open("wb") as stream:
            child = subprocess.Popen(
                argv, cwd=D, stdout=stream, stderr=subprocess.STDOUT
            )
            state.update(state="running", step=name, child=child.pid)
            save()
            code = child.wait()
        state["steps"].append(
            {
                "name": name,
                "argv": argv,
                "exit-code": code,
                "seconds": time.monotonic() - start,
                "log": log.name,
                **identity(log),
            }
        )
        save()
        assert code == 0, name
    state.update(
        state="finished",
        **{
            "exit-code": 0,
            "scope": "Final complete record audit after all three source-bound execution gates, including retained Windows byte identities. Full process emulation remains active.",
        }
    )
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    state["utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps(state), flush=True)
raise SystemExit(state["exit-code"])
