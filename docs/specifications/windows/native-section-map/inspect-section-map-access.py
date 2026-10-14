from pathlib import Path
import ctypes
import datetime
import hashlib
import json
import re
import struct
import subprocess
import sys

if not __debug__:
    raise RuntimeError("Optimized evidence inspection is unsupported")
TASK = Path("C:/Windows/Temp/assist-native-rax-20261010")
SHARE = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")


def require(ok, why):
    if not ok:
        raise ValueError(why)


def identity(path, pe=False):
    data = path.read_bytes()
    value = {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    if pe:
        require(data[:2] == b"MZ", "DOS signature")
        offset = struct.unpack_from("<I", data, 0x3C)[0]
        require(data[offset:offset + 4] == b"PE\0\0", "PE signature")
        value["machine"] = struct.unpack_from("<H", data, offset + 4)[0]
        value["characteristics"] = struct.unpack_from("<H", data, offset + 22)[0]
    return value


producers = []
outcomes = json.loads((SHARE / "native-section-map-access-outcomes.json").read_text(encoding="utf-8-sig"))
freeze = json.loads((SHARE / "access-source-freeze.json").read_text())
for profile in ("arm64", "x64", "x86", "x86-laa"):
    directory = TASK / ("section-map-" + profile)
    stem = "native-section-map-access"
    source = identity(directory / (stem + ".cpp"))
    executable = identity(directory / (stem + "-" + profile + ".exe"), True)
    stdout = identity(directory / (stem + "-" + profile + ".log"))
    text = (directory / (stem + "-" + profile + ".log")).read_text()
    rows = [dict(re.findall(r"([\w-]+)=([^\s]+)", line)) for line in text.splitlines() if line.startswith("case=")]
    require(len(rows) == 914 and [int(row["case"]) for row in rows] == list(range(914)), "complete ordered matrix")
    require("failure=" not in text and f"complete cases=914 lifetime=1 pointer-bytes={4 if profile.startswith('x86') else 8}" in text, "complete matrix footer")
    require(text.count("lifetime status=") == 1 and "closed=1" in text, "view after section close")
    require(executable["machine"] == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[profile], "executable machine")
    require(bool(executable["characteristics"] & 0x20) == (profile != "x86"), "executable LAA")
    require(source["sha256"] == freeze[stem + ".cpp"]["sha256"], "actual frozen source")
    outcome = next(value for value in outcomes if value["profile"] == profile)
    require(outcome["exit-code"] == 0 and outcome["source-hash"].lower() == source["sha256"], "executed source and exit")
    require(outcome["exe-sha256"].lower() == executable["sha256"] and outcome["exe-bytes"] == executable["bytes"], "executed binary identity")
    producers.append({"profile": profile, "source": source, "executable": executable, "stdout": stdout})
native = ctypes.c_ushort()
process = ctypes.c_ushort()
query = ctypes.windll.kernel32.IsWow64Process2
query.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ushort), ctypes.POINTER(ctypes.c_ushort)]
require(query(ctypes.c_void_p(-1), ctypes.byref(process), ctypes.byref(native)) != 0, "native machine query")
require(native.value == 0xAA64 and process.value == 0, "native inspector process")
libraries = [identity(Path("C:/Windows") / folder / name, True) for folder in ("System32", "SysWOW64") for name in ("ntdll.dll", "kernel32.dll")]
fresh = json.loads((SHARE / "native-section-map-fresh-identities.json").read_text())
for after, before in zip(libraries, fresh["libraries"]):
    require(after["path"] == before["path"] and after["sha256"] == before["sha256"] and after["bytes"] == before["bytes"], "native libraries unchanged")
result = {"captured-utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "kernel": list(sys.getwindowsversion()),
          "principal": subprocess.check_output(["whoami", "/user", "/fo", "csv", "/nh"], text=True).strip(),
          "native-machine": native.value, "process-machine": process.value,
          "python": sys.version, "inspector": identity(Path(__file__)),
          "libraries": libraries, "producers": producers}
(SHARE / "native-section-map-access-identities.json").write_text(json.dumps(result, indent=2) + "\n")
print("actual-map-source-exe-stdout-identities", len(producers))
