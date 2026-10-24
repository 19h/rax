"""Bound complete read-only GUI allocations and current-thread scalar inputs."""

from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import struct

nt = c.WinDLL("ntdll")
kernel = c.WinDLL("kernel32", use_last_error=True)
kernel.LoadLibraryW.argtypes = [w.LPCWSTR]
kernel.LoadLibraryW.restype = w.HMODULE
kernel.FreeLibrary.argtypes = [w.HMODULE]
nt.NtQueryInformationProcess.argtypes = [
    w.HANDLE,
    w.ULONG,
    c.c_void_p,
    w.ULONG,
    c.c_void_p,
]
nt.NtQueryInformationThread.argtypes = [
    w.HANDLE,
    w.ULONG,
    c.c_void_p,
    w.ULONG,
    c.c_void_p,
]
kernel.ReadProcessMemory.argtypes = [
    w.HANDLE,
    c.c_void_p,
    c.c_void_p,
    c.c_size_t,
    c.c_void_p,
]


class Region(c.Structure):
    _fields_ = [
        ("base", c.c_size_t),
        ("allocation", c.c_size_t),
        ("allocation-protection", w.DWORD),
        ("partition", w.DWORD),
        ("size", c.c_size_t),
        ("state", w.DWORD),
        ("protection", w.DWORD),
        ("kind", w.DWORD),
    ]


assert c.sizeof(Region) == 48 and c.sizeof(c.c_void_p) == 8
kernel.VirtualQuery.argtypes = [c.c_void_p, c.POINTER(Region), c.c_size_t]
kernel.VirtualQuery.restype = c.c_size_t


def read(address, length):
    assert 0 < length <= 16 << 20
    data = c.create_string_buffer(length)
    got = c.c_size_t()
    assert kernel.ReadProcessMemory(-1, address, data, length, c.byref(got))
    assert got.value == length
    return data.raw


def allocation(address):
    first = Region()
    assert kernel.VirtualQuery(address, c.byref(first), c.sizeof(first)) == 48
    base = first.allocation
    assert base and base % 65536 == 0
    rows = []
    cursor = base
    while cursor - base < 64 << 20:
        region = Region()
        assert kernel.VirtualQuery(cursor, c.byref(region), c.sizeof(region)) == 48
        if region.allocation != base:
            return {"base": base, "bytes": cursor - base, "regions": rows}
        assert region.base == cursor and region.size and region.size % 4096 == 0
        assert region.size <= (64 << 20) - (cursor - base)
        row = {key: getattr(region, key) for key, _ in Region._fields_}
        if region.state == 0x1000:
            assert region.protection == 2 and region.kind == 0x40000
            data = read(cursor, region.size)
            row.update(
                sha256=hashlib.sha256(data).hexdigest(),
                **{"nonzero-bytes": sum(x != 0 for x in data)}
            )
        else:
            assert region.state == 0x2000
        rows.append(row)
        cursor += region.size
    raise ValueError("GUI allocation exceeds 64 MiB")


user = kernel.LoadLibraryW("user32.dll")
assert user
process = c.create_string_buffer(48)
thread = c.create_string_buffer(48)
returned = w.ULONG()
assert nt.NtQueryInformationProcess(-1, 0, process, 48, c.byref(returned)) >= 0
assert returned.value == 48
assert nt.NtQueryInformationThread(-2, 0, thread, 48, c.byref(returned)) >= 0
assert returned.value == 48
peb = struct.unpack_from("<Q", process.raw, 8)[0]
teb = struct.unpack_from("<Q", thread.raw, 8)[0]
peb_bytes = read(peb, 512)
client = read(teb + 0x800, 0x100)
word = lambda data, at: struct.unpack_from("<Q", data, at)[0]
gdi = word(peb_bytes, 0xF8)
desktop = word(client, 0x28)
cti = word(client, 0x60)
result = {
    "state": "finished",
    "exit-code": 0,
    "peb": peb,
    "teb": teb,
    "current-thread-id": struct.unpack_from("<Q", thread.raw, 24)[0],
    "client-fields": [
        {"offset": at + 0x800, "value": word(client, at)} for at in range(0, 0xA0, 8)
    ],
    "gdi": allocation(gdi),
    "desktop": allocation(desktop),
    "client-thread-pointer": cti,
    "client-thread-first-32": read(cti, 32).hex(),
    "scope": "Full allocation extents and commitment hashes, not raw desktop contents; native ARM64 current-process metadata.",
}
assert kernel.FreeLibrary(user)
print(json.dumps(result), flush=True)
