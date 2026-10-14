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
        magic = struct.unpack_from("<H", data, offset + 24)[0]
        value["preferred-image-base"] = struct.unpack_from("<Q" if magic == 0x20B else "<I", data, offset + (48 if magic == 0x20B else 52))[0]
    return value


producers = []
stem = "native-section-map-fresh"
directory = TASK / "section-map-arm64"
log = directory / (stem + "-arm64.log")
text = log.read_text()
require("failure=" not in text and text.count("view-complete ") == 3, "fresh complete views")
require("complete cases=2 warmup=1" in text, "fresh complete cases")
for ordinal in (1, 2):
    case = re.search(rf"case={ordinal} warmup=0 installed=1 done=1 calls=1 exit=77 handles-before=(\d+) handles-after=(\d+) exact-handle-set=1", text)
    require(case is not None and case[1] == case[2], "fresh exact parent handle sets")
executable = identity(directory / (stem + "-arm64.exe"), True)
require(executable["machine"] == 0xAA64, "native producer machine")
producers.append({"family": "fresh", "profile": "arm64", "source": identity(directory / (stem + ".cpp")), "executable": executable, "stdout": identity(log)})
native = ctypes.c_ushort()
process = ctypes.c_ushort()
query = ctypes.windll.kernel32.IsWow64Process2
query.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ushort), ctypes.POINTER(ctypes.c_ushort)]
require(query(ctypes.c_void_p(-1), ctypes.byref(process), ctypes.byref(native)) != 0, "native machine query")
require(native.value == 0xAA64 and process.value == 0, "native inspector process")
libraries = [identity(Path("C:/Windows") / folder / name, True) for folder in ("System32", "SysWOW64") for name in ("ntdll.dll", "kernel32.dll")]
result = {
    "captured-utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "kernel": list(sys.getwindowsversion()),
    "principal": subprocess.check_output(["whoami", "/user", "/fo", "csv", "/nh"], text=True).strip(),
    "native-machine": native.value,
    "process-machine": process.value,
    "python": sys.version,
    "inspector": identity(Path(__file__)),
    "libraries": libraries,
    "producers": producers,
}
(SHARE / "native-section-map-fresh-identities.json").write_text(json.dumps(result, indent=2) + "\n")
print("actual-map-source-exe-stdout-identities", len(producers))
