"""Read task-owned synthetic SDBs with installed Apphelp APIs."""
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
from pathlib import Path
import struct

TASK = Path("C:/Windows/Temp/assist-native-rax-20261010/apphelp-sdb-padding")
TASK.mkdir(exist_ok=True)
api = C.WinDLL("C:/Windows/System32/apphelp.dll")


def function(name, result, arguments):
    value = getattr(api, name)
    value.restype = result
    value.argtypes = arguments
    return value


open_database = function("SdbOpenDatabase", C.c_void_p, [W.LPCWSTR, C.c_int])
close = function("SdbCloseDatabase", None, [C.c_void_p])
first = function("SdbGetFirstChild", W.DWORD, [C.c_void_p, W.DWORD])
next_child = function("SdbGetNextChild", W.DWORD, [C.c_void_p, W.DWORD, W.DWORD])
tag_from_id = function("SdbGetTagFromTagID", W.WORD, [C.c_void_p, W.DWORD])


def variable(tag, data):
    return struct.pack("<HI", tag, len(data)) + data


records = []
for tag, payload in [(0x9013, b"x"), (0x9013, b"ProductVersion=9."), (0x2001, b"\x7f")]:
    for padded in [False, True]:
        first_tag = variable(tag, payload) if tag >> 12 == 9 else struct.pack("<H", tag) + payload
        content = first_tag + (b"\xdb" if padded else b"") + struct.pack("<HI", 0x4032, 2) + variable(0x9010, bytes(64))
        data = struct.pack("<II4s", 3, 0, b"sdbf") + variable(0x7001, content) + variable(0x7801, b"")
        path = TASK / f"padding-{tag:04x}-{len(payload)}-{int(padded)}.sdb"
        path.write_bytes(data)
        handle = open_database(str(path), 0)
        if not handle:
            if padded:
                raise RuntimeError("padded synthetic SDB was rejected")
            records.append({"tag": tag, "payload-bytes": len(payload), "padded": padded, "file-sha256": hashlib.sha256(data).hexdigest(), "open": "rejected"})
            continue
        try:
            parent = first(handle, 0)
            child = first(handle, parent)
            sibling = next_child(handle, parent, child)
            actual = tag_from_id(handle, sibling) if sibling else 0
            if padded and actual != 0x4032:
                raise RuntimeError("padded next sibling is not expected DWORD")
            if not padded and actual == 0x4032:
                raise RuntimeError("unpadded next sibling unexpectedly aligned")
            records.append({"tag": tag, "payload-bytes": len(payload), "padded": padded, "file-bytes": len(data), "file-sha256": hashlib.sha256(data).hexdigest(), "parent-tagid": parent, "first-tagid": child, "next-tagid": sibling, "next-tag": actual})
        finally:
            close(handle)
output = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/native-sdb-padding.json")
output.write_text(json.dumps({"records": records, "script-sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}, indent=2) + "\n", encoding="utf-8")
print("synthetic read controls", len(records), "opened", sum("next-tag" in row for row in records))
