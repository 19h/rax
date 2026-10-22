"""Read-only independent fixed WNF registry and installed query observations."""

from pathlib import Path
import ctypes
import hashlib
import json
import platform
import sys
import winreg

S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
OUT = S / "native-wnf-provisioning-probe-v1.json"
assert not OUT.exists()
KEY = r"SYSTEM\CurrentControlSet\Control\Notifications"
NAMES = ["0280032EA3BC0875", "41911A28A3BC10B5", "0000000000000000"]


def registry():
    rows = []
    with winreg.OpenKey(
        winreg.HKEY_LOCAL_MACHINE, KEY, 0, winreg.KEY_QUERY_VALUE
    ) as key:
        for name in NAMES:
            try:
                raw, kind = winreg.QueryValueEx(key, name)
            except FileNotFoundError as error:
                assert error.winerror == 2
                rows.append({"name": name, "status": 2, "present": False})
            else:
                assert (
                    kind == winreg.REG_BINARY
                    and isinstance(raw, bytes)
                    and len(raw) <= 65536
                )
                rows.append(
                    {
                        "name": name,
                        "status": 0,
                        "present": True,
                        "kind": kind,
                        "bytes": len(raw),
                        "data-hex": raw.hex(),
                        "sha256": hashlib.sha256(raw).hexdigest(),
                    }
                )
    return rows


before = registry()
ntdll = ctypes.WinDLL("ntdll")
query = ntdll.NtQueryWnfStateData
query.argtypes = [
    ctypes.POINTER(ctypes.c_uint64),
    ctypes.c_void_p,
    ctypes.c_void_p,
    ctypes.POINTER(ctypes.c_uint32),
    ctypes.c_void_p,
    ctypes.POINTER(ctypes.c_uint32),
]
query.restype = ctypes.c_int32
observations = []
for name, typed in [("0280032EA3BC0875", True), ("41911A28A3BC10B5", False)]:
    state = ctypes.c_uint64(int(name, 16))
    stamp = ctypes.c_uint32(0xA5A5A5A5)
    length = ctypes.c_uint32(8 if typed else 0)
    guid = (ctypes.c_uint32 * 4).from_buffer_copy(
        bytes.fromhex("6364f948d652854e9c02f2c14e17264e")
    )
    data = (ctypes.c_uint8 * 8)(*([0xA5] * 8))
    status = query(
        ctypes.byref(state),
        ctypes.byref(guid) if typed else None,
        None,
        ctypes.byref(stamp),
        ctypes.byref(data) if typed else None,
        ctypes.byref(length),
    )
    observations.append(
        {
            "name": name,
            "status": hex(status & 0xFFFFFFFF),
            "change-stamp": stamp.value,
            "returned-bytes": length.value,
            "payload-hex": bytes(data).hex() if typed else None,
            "query-capacity-bytes": 8 if typed else 0,
        }
    )
after = registry()
assert before == after
source = Path(__file__).read_bytes()
result = {
    "state": "finished",
    "platform": platform.platform(),
    "machine": platform.machine(),
    "python": sys.version,
    "source": {
        "path": str(Path(__file__)),
        "bytes": len(source),
        "sha256": hashlib.sha256(source).hexdigest(),
    },
    "registry-before": before,
    "registry-after": after,
    "queries": observations,
    "scope": "Fixed read-only HKLM values and fixed installed NTDLL queries; no WNF write, subscription, impersonation, policy or system configuration change.",
}
OUT.write_text(json.dumps(result, indent=2) + "\n")
print(
    json.dumps(
        {
            "machine": result["machine"],
            "registry": [(r["name"], r["present"]) for r in before],
            "queries": observations,
        }
    )
)
