"""Receive a sealed stdin packet inside the exact native RAX tree."""

from pathlib import Path
import base64
import hashlib
import io
import json
import os
import subprocess
import sys
import zipfile

R = Path("C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
root = R / "target/win32u-validation-recovery-v2"
assert not root.exists() and root.parent.resolve().is_relative_to(R.resolve())
payload = base64.b64decode(sys.stdin.readline().strip(), validate=True)
assert len(payload) <= 32 << 20
root.mkdir()
transfer = root / "transfer"
transfer.mkdir()
rows = []
with zipfile.ZipFile(io.BytesIO(payload)) as archive:
    members = archive.infolist()
    assert len({row.filename for row in members}) == len(members) <= 64
    assert sum(row.file_size for row in members) <= 64 << 20
    for row in members:
        assert Path(row.filename).name == row.filename and not row.is_dir()
        data = archive.read(row)
        target = transfer / row.filename
        with target.open("xb") as output:
            output.write(data)
        assert target.read_bytes() == data
        rows.append(
            {
                "file": row.filename,
                "bytes": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        )
state = {
    "state": "received",
    "pid": os.getpid(),
    "packet-bytes": len(payload),
    "packet-sha256": hashlib.sha256(payload).hexdigest(),
    "files": rows,
    "children": [],
    "scope": "Local native transfer files replace inaccessible Home sharing. Frozen RAX source is not modified by this receiver.",
}
record = root / "deployment.json"
record.write_text(json.dumps(state, indent=2) + "\n")
for name in [
    "capture-win32u-runtime-inputs-v2.py",
    "run-win32u-services-candidate2-windows.py",
    "run-win32u-services-native-owning-candidate2.py",
    "run-native-win32u-next-frontier-v3.py",
    "run-win32u-services-owning-capi-candidate3.py",
]:
    with (root / (name + ".bootstrap.log")).open("wb") as output:
        child = subprocess.Popen(
            [sys.executable, "-I", "-B", str(transfer / name)],
            cwd=R,
            stdin=subprocess.DEVNULL,
            stdout=output,
            stderr=subprocess.STDOUT,
            creationflags=subprocess.DETACHED_PROCESS
            | subprocess.CREATE_NEW_PROCESS_GROUP,
        )
    state["children"].append({"script": name, "pid": child.pid})
    record.write_text(json.dumps(state, indent=2) + "\n")
state.update(state="finished", **{"exit-code": 0})
record.write_text(json.dumps(state, indent=2) + "\n")
print(json.dumps(state), flush=True)
