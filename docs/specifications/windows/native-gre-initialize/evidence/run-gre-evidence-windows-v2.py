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
import tarfile
from pathlib import PurePosixPath

S = Path(r"C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154")
R = Path(r"C:\Windows\Temp\assist-native-rax-20261010\src-display-candidate1")
D = R / "docs/specifications/windows/native-gre-initialize"
P = S / "gre-initialize-evidence-transfer-v2-source.json"
A = S / "gre-initialize-evidence-transfer-v2.tar.gz"
F = S / "gre-initialize-evidence-windows-v2.json"
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
    state["driver-before"] = identity(Path(__file__))
    state["metadata-before"] = identity(P)
    state["archive-before"] = identity(A)
    metadata = json.loads(P.read_bytes())

    def pair(row):
        return row["bytes"], row["sha256"]

    assert pair(state["archive-before"]) == pair(metadata["archive"])
    with tarfile.open(A, "r:gz") as bundle:
        for member in bundle.getmembers():
            path = PurePosixPath(member.name)
            assert not path.is_absolute() and ".." not in path.parts
            assert member.isfile() and member.name.startswith(
                "docs/specifications/windows/native-gre-initialize/"
            )
            target = R.joinpath(*path.parts)
            assert os.path.commonpath([str(target.resolve()), str(D.resolve())]) == str(
                D.resolve()
            )
            raw = bundle.extractfile(member).read()
            if target.exists():
                assert target.read_bytes() == raw, member.name
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
    assert pair(identity(D / "check_evidence.py")) == pair(metadata["checker"])
    assert pair(identity(D / "proof.json")) == pair(metadata["proof"])
    for field, name in [("checker", "check_evidence.py"), ("proof", "proof.json")]:
        state[field + "-before"] = identity(D / name)
    log = S / "gre-initialize-evidence-windows-v2.log"
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
    for field, path in [("driver", Path(__file__)), ("metadata", P), ("archive", A)]:
        state[field + "-after"] = identity(path)
        assert state[field + "-before"] == state[field + "-after"], field
    assert child.returncode == 0
    state.update(state="finished", **{"exit-code": 0})
except BaseException:
    state.update(state="failed", error=traceback.format_exc(), **{"exit-code": 1})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
raise SystemExit(state["exit-code"])
