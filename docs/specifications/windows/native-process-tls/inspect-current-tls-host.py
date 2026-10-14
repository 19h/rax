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


fresh = json.loads((SHARE / 'native-process-tls-initial-identities.json').read_text())
libraries = [identity(Path(item['path']), True) for item in fresh['libraries']]
for after, before in zip(libraries, fresh['libraries']):
    require(all(after[key] == before[key] for key in ['path', 'bytes', 'sha256', 'machine', 'characteristics']), 'final native library identity')
native = ctypes.c_ushort()
process = ctypes.c_ushort()
query = ctypes.windll.kernel32.IsWow64Process2
query.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ushort), ctypes.POINTER(ctypes.c_ushort)]
require(query(ctypes.c_void_p(-1), ctypes.byref(process), ctypes.byref(native)) != 0, 'native machine query')
principal = subprocess.check_output(['whoami', '/user', '/fo', 'csv', '/nh'], text=True).strip()
require(list(sys.getwindowsversion()) == fresh['kernel'] and principal == fresh['principal'] and native.value == 0xAA64 and process.value == 0, 'final owning native environment')
result = {'captured-utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'kernel': list(sys.getwindowsversion()), 'principal': principal, 'native-machine': native.value, 'process-machine': process.value, 'libraries': libraries, 'inspector': identity(Path(__file__)), 'same-as-original-native-captures': True}
(SHARE / 'native-process-tls-final-host-identity.json').write_text(json.dumps(result, indent=2) + '\n')
print('final native environment and four library identities matched')
