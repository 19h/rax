"""Retire only verified legacy task scripts blocked on the disabled Home path."""

import ctypes as c
from ctypes import wintypes as w
from pathlib import Path
import json

k = c.WinDLL("kernel32", use_last_error=True)
n = c.WinDLL("ntdll")


class Process(c.Structure):
    _fields_ = [
        ("size", w.DWORD),
        ("usage", w.DWORD),
        ("pid", w.DWORD),
        ("heap", c.c_size_t),
        ("module", w.DWORD),
        ("threads", w.DWORD),
        ("parent", w.DWORD),
        ("priority", w.LONG),
        ("flags", w.DWORD),
        ("name", w.WCHAR * 260),
    ]


class Unicode(c.Structure):
    _fields_ = [("length", w.USHORT), ("maximum", w.USHORT), ("pointer", c.c_void_p)]


k.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
k.CreateToolhelp32Snapshot.restype = w.HANDLE
k.Process32FirstW.argtypes = [w.HANDLE, c.POINTER(Process)]
k.Process32NextW.argtypes = [w.HANDLE, c.POINTER(Process)]
k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
k.OpenProcess.restype = w.HANDLE
k.QueryFullProcessImageNameW.argtypes = [
    w.HANDLE,
    w.DWORD,
    w.LPWSTR,
    c.POINTER(w.DWORD),
]
k.TerminateProcess.argtypes = [w.HANDLE, w.UINT]
k.CloseHandle.argtypes = [w.HANDLE]
n.NtQueryInformationProcess.argtypes = [
    w.HANDLE,
    w.ULONG,
    c.c_void_p,
    w.ULONG,
    c.POINTER(w.ULONG),
]
n.NtQueryInformationProcess.restype = w.LONG

names = {
    "run-win32u-services-candidate1-windows.py",
    "run-win32u-services-native-owning-candidate1.py",
    "run-native-win32u-next-frontier-v1.py",
    "run-native-win32u-next-frontier-v2.py",
    "run-win32u-services-owning-capi-candidate1.py",
    "run-win32u-services-owning-capi-candidate2.py",
    "capture-win32u-runtime-inputs-v1.py",
}
prefix = "c:/mac/home/downloads/assist-native-allocate-ex-transfer-20261010-2154/"
snapshot = k.CreateToolhelp32Snapshot(2, 0)
assert snapshot not in [None, c.c_void_p(-1).value]
p = Process()
p.size = c.sizeof(p)
ok = k.Process32FirstW(snapshot, c.byref(p))
rows = []
while ok:
    if p.name.lower() in ["python.exe", "pythonw.exe"]:
        handle = k.OpenProcess(0x1001, False, p.pid)
        if handle:
            path = c.create_unicode_buffer(32768)
            length = w.DWORD(32768)
            if k.QueryFullProcessImageNameW(
                handle, 0, path, c.byref(length)
            ) and path.value.replace(chr(92), "/").lower().startswith(
                "c:/windows/temp/assist-native-rax-20261010/"
            ):
                size = w.ULONG()
                n.NtQueryInformationProcess(handle, 60, None, 0, c.byref(size))
                if 0 < size.value <= 65536:
                    buf = c.create_string_buffer(size.value)
                    status = n.NtQueryInformationProcess(
                        handle, 60, buf, len(buf), c.byref(size)
                    )
                    u = Unicode.from_buffer(buf)
                    if (
                        status >= 0
                        and u.pointer
                        and u.length % 2 == 0
                        and c.addressof(buf) <= u.pointer
                        and u.pointer + u.length <= c.addressof(buf) + len(buf)
                    ):
                        command = c.wstring_at(u.pointer, u.length // 2)
                        normalized = command.replace(chr(92), "/").lower()
                        matched = next(
                            (name for name in names if prefix + name in normalized),
                            None,
                        )
                        if matched:
                            assert k.TerminateProcess(
                                handle, 0xE0000002
                            ), c.get_last_error()
                            rows.append(
                                {
                                    "pid": p.pid,
                                    "parent": p.parent,
                                    "image": path.value,
                                    "command-before": command,
                                    "verified-legacy-script": matched,
                                    "exit-request": "0xe0000002",
                                    "reason": "Replaced by sealed local-disk recovery after Home sharing became unavailable; original partial receipts retained.",
                                }
                            )
            k.CloseHandle(handle)
    ok = k.Process32NextW(snapshot, c.byref(p))
k.CloseHandle(snapshot)
result = {
    "state": "finished",
    "exit-code": 0,
    "retired": rows,
    "scope": "Only exact legacy script paths and pinned task Python image prefix; new local recovery processes excluded by path.",
}
target = Path(
    "C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1/target/win32u-validation-recovery-v2/retired-legacy-processes-v1.json"
)
target.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result), flush=True)
