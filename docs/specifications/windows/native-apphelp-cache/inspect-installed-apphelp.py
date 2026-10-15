"""Read fixed installed AppPatch inputs; emit hashes and narrow name records."""
import hashlib
import json
import os
from pathlib import Path


def identity(path):
    data = path.read_bytes()
    return data, {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


root = Path(os.environ["SystemRoot"])
records = []
for path in sorted((root / "AppPatch").rglob("*.sdb")):
    if path.stat().st_size > 64 * 1024 * 1024:
        raise RuntimeError("installed database exceeds inspection bound")
    data, record = identity(path)
    record["header"] = data[:12].hex()
    record["names"] = []
    for name in ["apphelp.dll", "msvcrt.dll", "kernelbase.dll"]:
        for spelling in [name, name.upper()]:
            needle = spelling.encode("utf-16-le")
            offset = 0
            while (offset := data.find(needle, offset)) >= 0:
                record["names"].append({"name": spelling, "offset": offset, "before-32": data[max(0, offset - 32):offset].hex(), "after-48": data[offset + len(needle):offset + len(needle) + 48].hex()})
                offset += len(needle)
    records.append(record)
for relative in ["System32/drivers/ahcache.sys", "System32/ntdll.dll", "SysWOW64/ntdll.dll"]:
    path = root / relative
    if path.exists():
        data, record = identity(path)
        if path.suffix == ".sys":
            import re
            record["utf16-candidates"] = [m.group().decode("utf-16-le") for m in re.finditer(rb"(?:[\x20-\x7E]\x00){6,}", data) if any(word in m.group().lower() for word in [b"d\x00l\x00l\x00", b"s\x00d\x00b\x00", b"c\x00d\x00b\x00"])]
        records.append(record)
output = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/installed-apphelp-inspection.json")
output.write_text(json.dumps({"records": records}, indent=2) + "\n", encoding="utf-8")
print("records", len(records), "output-sha256", hashlib.sha256(output.read_bytes()).hexdigest())
