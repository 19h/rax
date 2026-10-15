"""Independently inspect native producer files and terminal outcomes."""
from pathlib import Path
import ctypes
import datetime
import hashlib
import json
import re
import struct
import subprocess
import sys

TASK = Path("C:/Windows/Temp/assist-native-rax-20261010")
SHARE = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")


def require(value, reason):
    if not value:
        raise ValueError(reason)


def identity(path, executable=False):
    data = path.read_bytes()
    result = {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    if executable:
        require(data[:2] == b"MZ", "DOS signature")
        pe = struct.unpack_from("<I", data, 0x3C)[0]
        require(data[pe:pe + 4] == b"PE\0\0", "PE signature")
        result["machine"] = struct.unpack_from("<H", data, pe + 4)[0]
        result["characteristics"] = struct.unpack_from("<H", data, pe + 22)[0]
    return result


producers = []
for group in ["lookup", "offsets", "output-offsets", "names", "cdb-names", "buffers", "ordering"]:
    outcomes = json.loads((SHARE / f"native-apphelp-{group}-outcomes.json").read_text(encoding="utf-8-sig"))
    for profile in ["arm64", "x64", "x86", "x86-laa"]:
        directory = TASK / f"apphelp-{group}-{profile}"
        stem = f"native-apphelp-{group}-{profile}"
        source = identity(directory / f"native-apphelp-{group}.cpp")
        expected_source = identity(SHARE / f"native-apphelp-{group}.cpp")
        require(source["sha256"] == expected_source["sha256"] and source["bytes"] == expected_source["bytes"], "actual native producer source")
        executable = identity(directory / f"{stem}.exe", True)
        stdout = identity(directory / f"{stem}.log")
        outcome = next(row for row in outcomes if row["profile"] == profile)
        require(outcome["exit-code"] == 0 and outcome["source-hash"].lower() == source["sha256"], "recorded source and exit")
        require(outcome["exe-sha256"].lower() == executable["sha256"] and outcome["exe-bytes"] == executable["bytes"], "recorded executable identity")
        require(executable["machine"] == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[profile], "native executable ISA")
        require(bool(executable["characteristics"] & 0x20) == (profile != "x86"), "actual LAA characteristic")
        text = (directory / f"{stem}.log").read_text()
        rows = [dict(re.findall(r"([\w-]+)=([^ ]*)", line)) for line in text.splitlines() if line.startswith("case=")]
        expected_count = {"lookup": 16, "offsets": 15 if profile.startswith("x86") else 11, "output-offsets": 210 if profile.startswith("x86") else 11, "names": 25, "cdb-names": 167, "buffers": 101, "ordering": 36}[group]
        require(len(rows) == expected_count and [int(row["case"]) for row in rows] == list(range(expected_count)), "complete ordered native rows")
        require(all(row.get("escaped", "00000000") == "00000000" for row in rows), "escaped native exception")
        require("failure=" not in text and text.splitlines()[-1].startswith("complete"), "native producer completion")
        require(stdout["sha256"] == identity(SHARE / f"{stem}.log")["sha256"], "transferred raw output")
        producers.append({"group": group, "profile": profile, "source": source, "executable": executable, "stdout": stdout, "cases": expected_count})

libraries = [identity(Path("C:/Windows") / folder / name, True) for folder in ["System32", "SysWOW64"] for name in ["ntdll.dll", "kernel32.dll"]]
baseline = json.loads((TASK / "src/docs/specifications/windows/native-process-tls/native-process-tls-initial-identities.json").read_text())
require(libraries == baseline["libraries"], "unchanged physical libraries since TLS native controls")
native = ctypes.c_ushort()
process = ctypes.c_ushort()
query = ctypes.windll.kernel32.IsWow64Process2
query.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ushort), ctypes.POINTER(ctypes.c_ushort)]
require(query(ctypes.c_void_p(-1), ctypes.byref(process), ctypes.byref(native)) != 0 and native.value == 0xAA64 and process.value == 0, "native inspector architecture")
result = {"captured-utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "kernel": list(sys.getwindowsversion()), "principal": subprocess.check_output(["whoami", "/user", "/fo", "csv", "/nh"], text=True).strip(), "native-machine": native.value, "process-machine": process.value, "libraries": libraries, "inspector": identity(Path(__file__)), "producers": producers, "total-cases": sum(row["cases"] for row in producers), "replayed-cases": 1316, "scope": "post-run actual producer inspection; physical libraries match preceding TLS native controls"}
output = SHARE / "native-apphelp-identities.json"
output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print("inspected producers", len(producers), "native cases", result["total-cases"])
