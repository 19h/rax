import hashlib
import json
import os
from pathlib import Path
import struct


def entries(data, start, end):
    while start < end:
        tag = struct.unpack_from("<H", data, start)[0]
        kind = tag >> 12
        if kind in (1, 2, 3, 4, 5, 6):
            count = {1: 0, 2: 1, 3: 2, 4: 4, 5: 8, 6: 4}[kind]
            payload = start + 2
        elif kind in (7, 8, 9):
            count = struct.unpack_from("<I", data, start + 2)[0]
            payload = start + 6
        else:
            raise RuntimeError(f"unknown tag at {start:x}: {tag:x}")
        if payload + count > end:
            raise RuntimeError("tag exceeds owning list")
        yield tag, start, payload, payload + count
        start = (payload + count + 1) & ~1
    assert start == end


root = Path(os.environ["SystemRoot"]) / "AppPatch"
records = []


def validate(data, start, end, ancestry):
    try:
        for tag, at, payload, extent in entries(data, start, end):
            if tag >> 12 == 7:
                error = validate(data, payload, extent, ancestry + [{"tag": f"{tag:04X}", "offset": at, "extent": extent}])
                if error:
                    return error
    except Exception as error:
        return {"error": str(error), "start": start, "end": end, "ancestry": ancestry, "prefix-64": data[start:min(end,start+64)].hex()}
    return None

for path in [root / "sysmain.sdb", root / "01DD580459E03375.sysmain.sdb"]:
    data = path.read_bytes()
    roots = list(entries(data, 12, len(data)))
    table = next(t for t in roots if t[0] == 0x7801)
    strings = {start - table[1]: data[payload:end] for _, start, payload, end in entries(data, table[2], table[3])}
    database = next(t for t in roots if t[0] == 0x7001)
    names = {}
    unresolved = []
    for tag, start, payload, end in entries(data, database[2], database[3]):
        if tag != 0x7007:
            continue
        fields = list(entries(data, payload, end))
        name = next((t for t in fields if t[0] == 0x6001), None)
        flags = next((t for t in fields if t[0] == 0x4032), None)
        if not name or not flags:
            continue
        reference = struct.unpack_from("<I", data, name[2])[0]
        if reference not in strings:
            unresolved.append({"exe": start, "reference": reference})
            continue
        text = strings[reference].decode("utf-16-le").removesuffix("\0")
        value = struct.unpack_from("<I", data, flags[2])[0]
        names[text] = names.get(text, 0) | value
    records.append({"path": str(path), "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data), "roots": roots, "strings": len(strings), "candidates": names, "unresolved": unresolved, "validation-error": validate(data,12,len(data),[])})
output = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/sdb-file-structure.json")
output.write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
print("records", len(records), "output-sha256", hashlib.sha256(output.read_bytes()).hexdigest())
