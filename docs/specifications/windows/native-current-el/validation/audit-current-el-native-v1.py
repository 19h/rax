"""Read-only live identity audit; no policy or Defender changes."""

from pathlib import Path
import datetime
import hashlib
import json
import platform
import sys
import traceback

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
F = S / "current-el-native-live-audit-v1.json"
assert not F.exists()
state = {
    "state": "preparing",
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
}


def identity(p):
    h = hashlib.sha256()
    n = 0
    with p.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
            n += len(block)
    return {"path": str(p), "bytes": n, "sha256": h.hexdigest()}


try:
    a = json.loads((S / "native-current-el-v3-launch.json").read_text())
    assert a["state"] == "finished" and a["exit-code"] == 0
    rows = []
    for key in ["source", "executable", "modules"]:
        assert a[key + "-before"] == a[key + "-after"]
        items = a[key + "-after"] if key == "modules" else [a[key + "-after"]]
        for x in items:
            y = identity(Path(x["path"]))
            assert y == x
            rows.append(y)
    retained = json.loads(
        (S / "file-section-retained-owning-candidate1.json").read_text()
    )
    copies = []
    for row in retained["files"]:
        y = identity(Path(row["retained"]))
        assert (y["bytes"], y["sha256"]) == (row["bytes"], row["sha256"])
        copies.append(y)
    assert len(copies) == 4
    version = sys.getwindowsversion()
    state.update(
        {
            "state": "verified",
            "exit-code": 0,
            "machine": platform.machine(),
            "windows-version": {
                "major": version.major,
                "minor": version.minor,
                "build": version.build,
                "platform-version": list(version.platform_version),
            },
            "native-producer-live-identities": rows,
            "previous-owning-retained-identities": copies,
            "native-launch": identity(S / "native-current-el-v3-launch.json"),
            "native-log": identity(S / "native-current-el-v3-native.log"),
            "driver": identity(S / "run-native-current-el-v3.py"),
        }
    )
except BaseException:
    state.update({"state": "failed", "exit-code": 1, "error": traceback.format_exc()})
F.write_text(json.dumps(state, indent=2) + "\n")
print(state["state"], flush=True)
raise SystemExit(state["exit-code"])
