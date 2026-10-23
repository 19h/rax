from pathlib import Path
import hashlib, json, os, datetime

S = Path(
    "C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1/target/win32u-validation-recovery-v2/transfer"
)
source = S / "win32u-runtime-paths-v1.json"
out = S / "win32u-runtime-inputs-v2.json"
assert not out.exists()
paths = {}
for name in json.loads(source.read_text())["paths"]:
    p = Path(name)
    normalized = str(p).removeprefix("\\\\?\\").replace("/", "\\").casefold()
    assert normalized.startswith(("c:\\windows\\system32\\", "c:\\windows\\syswow64\\"))
    paths[normalized] = p


def identity(p):
    b = p.read_bytes()
    assert p.is_file() and len(b) <= 64 << 20
    return {"path": str(p), "bytes": len(b), "sha256": hashlib.sha256(b).hexdigest()}


rows = [identity(paths[n]) for n in sorted(paths)]
assert rows == [identity(paths[n]) for n in sorted(paths)]
r = {
    "state": "finished",
    "exit-code": 0,
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "path-source": {
        "file": source.name,
        "bytes": source.stat().st_size,
        "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    },
    "runtime-inputs": rows,
    "scope": "Canonical deduplicated actual parent ordinary guest module paths, native four-view Win32u producer libraries, and version.dll; read-only stable before/after capture.",
}
out.write_text(json.dumps(r, indent=2) + "\n")
print(json.dumps({"state": "finished", "count": len(rows)}), flush=True)
