"""Use documented read-only SDB APIs on fixed installed databases."""
import ctypes as C
from ctypes import wintypes as W
from collections import Counter
import hashlib
import json
import os
from pathlib import Path


library = C.WinDLL(str(Path(os.environ["SystemRoot"]) / "System32/apphelp.dll"))


def api(name, result, arguments):
    function = getattr(library, name)
    function.restype = result
    function.argtypes = arguments
    return function


open_database = api("SdbOpenDatabase", C.c_void_p, [W.LPCWSTR, C.c_int])
close_database = api("SdbCloseDatabase", None, [C.c_void_p])
first = api("SdbGetFirstChild", W.DWORD, [C.c_void_p, W.DWORD])
next_child = api("SdbGetNextChild", W.DWORD, [C.c_void_p, W.DWORD, W.DWORD])
tag_from_id = api("SdbGetTagFromTagID", W.WORD, [C.c_void_p, W.DWORD])
tag_name = api("SdbTagToString", W.LPCWSTR, [W.WORD])
read_text = api("SdbReadStringTag", W.BOOL, [C.c_void_p, W.DWORD, W.LPWSTR, W.DWORD])
read_dword = api("SdbReadDWORDTag", W.DWORD, [C.c_void_p, W.DWORD, W.DWORD])
result = []
for basename in ["sysmain.sdb", "frxmain.sdb", "drvmain.sdb"]:
    path = Path(os.environ["SystemRoot"]) / "AppPatch" / basename
    database = open_database(str(path), 0)
    if not database:
        raise RuntimeError(f"cannot open fixed installed database {basename}")
    raw_file = path.read_bytes()
    counts = Counter()
    records = []
    lookup_candidates = []

    def walk(parent, ancestry):
        children = []
        identifier = first(database, parent)
        while identifier:
            tag = tag_from_id(database, identifier)
            node = {"offset": identifier, "tag": f"{tag:04X}", "tag-name": tag_name(tag)}
            counts[node["tag-name"]] += 1
            if sum(counts.values()) > 1000000 or len(ancestry) > 32:
                raise RuntimeError("database traversal exceeded bounds")
            if tag & 0xF000 in (0x6000, 0x8000):
                buffer = C.create_unicode_buffer(8192)
                if read_text(database, identifier, buffer, len(buffer)):
                    node["text"] = buffer.value
                    node["raw-tag-hex"] = raw_file[identifier:identifier+6].hex()
            elif tag & 0xF000 == 0x4000:
                node["dword"] = read_dword(database, identifier, 0xDEADBEEF)
            children.append(node)
            if tag & 0xF000 == 0x7000 and tag != 0x7801:
                walk(identifier, ancestry + [node])
            identifier = next_child(database, parent, identifier)
        if ancestry and ancestry[-1]["tag"] == "7007" and any(node["tag"] == "4032" for node in children):
            lookup_candidates.append({"ancestry": ancestry, "children": children})
        if len(ancestry) == 2 and ancestry[-1]["tag"] == "7002":
            lookup_candidates.append({"global-library": ancestry, "children": children})
        if len(ancestry) == 3 and ancestry[-2]["tag"] == "7002" and ancestry[-1]["tag"] == "7003":
            lookup_candidates.append({"global-module": ancestry, "children": children})
        if any(node.get("text", "").lower() in ["apphelp.dll", "msvcrt.dll", "kernelbase.dll"] for node in children):
            records.append({"ancestry": ancestry, "children": children})
        if len(ancestry) <= 1:
            records.append({"top-ancestry": ancestry, "children": children})

    try:
        walk(0, [])
    finally:
        close_database(database)
    result.append({"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "counts": dict(counts), "records": records, "lookup-candidates": lookup_candidates})
output = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/installed-sdb-api-inspection.json")
output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print("databases", len(result), "output-sha256", hashlib.sha256(output.read_bytes()).hexdigest())
