"""Measure NtCallbackReturn without an active callback in this native process."""

from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import sys

root = Path(
    "C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1/target/native-callback-return-v1"
)
assert not root.exists()
root.mkdir()
kernel = c.WinDLL("kernel32", use_last_error=True)
nt = c.WinDLL("ntdll")
kernel.GetCurrentProcess.restype = w.HANDLE
kernel.GetLastError.restype = w.DWORD
kernel.SetLastError.argtypes = [w.DWORD]
kernel.VirtualAlloc.argtypes = [c.c_void_p, c.c_size_t, w.DWORD, w.DWORD]
kernel.VirtualAlloc.restype = c.c_void_p
kernel.VirtualFree.argtypes = [c.c_void_p, c.c_size_t, w.DWORD]
kernel.GetProcessHandleCount.argtypes = [w.HANDLE, c.POINTER(w.DWORD)]
nt.NtCallbackReturn.argtypes = [c.c_void_p, w.ULONG, w.LONG]
nt.NtCallbackReturn.restype = w.LONG


def identity(path):
    raw = path.read_bytes()
    return {
        "file": str(path),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
    }


def handles():
    value = w.DWORD()
    assert kernel.GetProcessHandleCount(kernel.GetCurrentProcess(), c.byref(value))
    return value.value


module = Path("C:/Windows/System32/ntdll.dll")
result = {
    "state": "running",
    "python": sys.version,
    "pointer-bytes": c.sizeof(c.c_void_p),
    "python-image-before": identity(Path(sys.executable)),
    "ntdll-before": identity(module),
    "handle-count-before": handles(),
    "cases": [],
}
storage = c.create_string_buffer(64)
guard = kernel.VirtualAlloc(None, 4096, 0x3000, 1)
assert guard
for label, address in [
    ("null", 0),
    ("invalid", 1),
    ("readable", c.addressof(storage)),
    ("readable-unaligned", c.addressof(storage) + 1),
    ("noaccess", guard),
]:
    for length in [0, 1, 64, 0xFFFFFFFF]:
        for status in [0, 0x103, 0x80000001, 0xC0000001]:
            kernel.SetLastError(0x12345678)
            returned = nt.NtCallbackReturn(address, length, status) & 0xFFFFFFFF
            error = kernel.GetLastError()
            result["cases"].append(
                {
                    "buffer-class": label,
                    "length": length,
                    "input-status": status,
                    "returned-status": returned,
                    "last-error": error,
                }
            )
assert kernel.VirtualFree(guard, 0, 0x8000)
result.update(
    {
        "state": "finished",
        "exit-code": 0,
        "cases-recorded": len(result["cases"]),
        "handle-count-after": handles(),
        "python-image-after": identity(Path(sys.executable)),
        "ntdll-after": identity(module),
        "private-page-released": True,
    }
)
assert result["python-image-before"] == result["python-image-after"]
assert result["ntdll-before"] == result["ntdll-after"]
assert result["handle-count-before"] == result["handle-count-after"]
(root / "native-callback-return-v1.json").write_text(
    json.dumps(result, indent=2) + "\n"
)
print(json.dumps(result), flush=True)
