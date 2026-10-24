#!/usr/bin/env python3
"""Observe the selected Windows clock/NLS profile before emulator regressions.

This uses fixed own-process queries and reports module-relative code, scalar
outputs and content hashes. It neither admits a RAX build profile nor changes
service decoding. Full NLS fault/alias/protection qualification remains separate.
"""

import ctypes as c
import hashlib
import json
from pathlib import Path
import struct
import sys

MAX_IMAGE_BYTES = 64 << 20
MAX_NLS_BYTES = 8 << 20


def bind(library, name, result, *arguments):
    function = getattr(library, name)
    function.restype = result
    function.argtypes = arguments
    return function


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def emit(kind, **fields):
    print(json.dumps({"kind": kind, **fields}, sort_keys=True), flush=True)


def code_chain(read, image_bytes, machine, entry):
    """Bounded diagnostic traversal; never a trusted service decoder."""
    rows, seen = [], set()
    cursor = entry
    for _ in range(4):
        require(0 <= cursor < image_bytes, "branch leaves selected image")
        require(cursor not in seen, "cyclic export branch")
        seen.add(cursor)
        data = read(cursor, min(64, image_bytes - cursor))
        row = {"rva": cursor, "hex": data.hex()}
        target = None
        if machine in (0x14C, 0x8664) and data[:1] == b"\xe9":
            require(len(data) >= 5, "truncated x86 branch")
            target = cursor + 5 + struct.unpack_from("<i", data, 1)[0]
        elif machine == 0xAA64 and len(data) >= 4:
            word = struct.unpack_from("<I", data)[0]
            if word & 0xFC000000 == 0x14000000:
                displacement = word & 0x3FFFFFF
                if displacement & 0x2000000:
                    displacement -= 0x4000000
                target = cursor + displacement * 4
        rows.append(row)
        if target is None:
            return rows
        row["branch_rva"] = target
        cursor = target
    raise RuntimeError("export branch exceeds four-hop diagnostic bound")


class Native:
    def __init__(self):
        require(sys.platform == "win32", "native Windows process required")
        self.kernel = c.WinDLL("kernel32", use_last_error=True)
        self.ntdll = c.WinDLL("ntdll")
        pointer, size, uint = c.c_void_p, c.c_size_t, c.c_uint32
        self.read_memory = bind(
            self.kernel,
            "ReadProcessMemory",
            c.c_int,
            pointer,
            pointer,
            pointer,
            size,
            c.POINTER(size),
        )
        module = bind(self.kernel, "GetModuleHandleW", pointer, c.c_wchar_p)
        self.base = module("ntdll.dll")
        require(bool(self.base), "selected NTDLL is absent")
        header = self.read(self.base, 4096)
        require(header[:2] == b"MZ", "NTDLL DOS header")
        pe = struct.unpack_from("<I", header, 0x3C)[0]
        require(pe <= len(header) - 84, "bounded NTDLL PE header")
        require(header[pe : pe + 4] == b"PE\0\0", "NTDLL PE signature")
        self.machine = struct.unpack_from("<H", header, pe + 4)[0]
        magic = struct.unpack_from("<H", header, pe + 24)[0]
        require(
            (self.machine, magic, c.sizeof(pointer))
            in ((0x14C, 0x10B, 4), (0x8664, 0x20B, 8), (0xAA64, 0x20B, 8)),
            "native image and Python ABI disagree",
        )
        self.image_bytes = struct.unpack_from("<I", header, pe + 24 + 56)[0]
        require(4096 <= self.image_bytes <= MAX_IMAGE_BYTES, "NTDLL image bound")
        self.resolve = bind(self.kernel, "GetProcAddress", pointer, pointer, c.c_char_p)

    def read(self, address, length):
        require(0 < length <= MAX_NLS_BYTES, "native read bound")
        output, returned = (c.c_ubyte * length)(), c.c_size_t()
        ok = self.read_memory(-1, address, output, length, c.byref(returned))
        if not ok:
            raise c.WinError(c.get_last_error())
        require(returned.value == length, "short native read")
        return bytes(output)

    def code(self, name):
        address = self.resolve(self.base, name.encode("ascii"))
        require(bool(address), "selected export absent: " + name)
        rows = code_chain(
            lambda rva, length: self.read(self.base + rva, length),
            self.image_bytes,
            self.machine,
            address - self.base,
        )
        emit("code", export=name, chain=rows)

    def clocks(self):
        query = bind(self.ntdll, "NtQuerySystemTime", c.c_int32, c.POINTER(c.c_int64))
        precise = bind(
            self.kernel, "GetSystemTimePreciseAsFileTime", None, c.POINTER(c.c_uint64)
        )
        for ordinal in range(3):
            before, native, after = c.c_uint64(), c.c_int64(-1), c.c_uint64()
            precise(c.byref(before))
            status = query(c.byref(native))
            shared = None
            # KSYSTEM_TIME's two high words must agree. Bounded sampling avoids
            # an unbounded spin if the selected layout or update protocol differs.
            for _ in range(8):
                high1 = struct.unpack("<I", self.read(0x7FFE0018, 4))[0]
                low = struct.unpack("<I", self.read(0x7FFE0014, 4))[0]
                high2 = struct.unpack("<I", self.read(0x7FFE001C, 4))[0]
                if high1 == high2:
                    shared = low | (high1 << 32)
                    break
            precise(c.byref(after))
            emit(
                "clock",
                ordinal=ordinal,
                unit="100 ns since 1601-01-01 UTC",
                before=before.value,
                native=native.value,
                status=f"{status & 0xFFFFFFFF:08x}",
                shared=shared,
                after=after.value,
            )
            require(status >= 0 and shared is not None, "native clock query failed")

    def nls(self):
        pointer, uint = c.c_void_p, c.c_uint32

        class Region(c.Structure):
            _fields_ = [
                ("base", pointer),
                ("allocation", pointer),
                ("allocation_protection", uint),
                *([("partition_and_padding", uint)] if c.sizeof(pointer) == 8 else []),
                ("size", c.c_size_t),
                ("state", uint),
                ("protection", uint),
                ("kind", uint),
            ]

        require(c.sizeof(Region) == (48 if c.sizeof(pointer) == 8 else 28), "MBI ABI")
        query = bind(
            self.kernel,
            "VirtualQuery",
            c.c_size_t,
            pointer,
            c.POINTER(Region),
            c.c_size_t,
        )
        unmap = bind(self.ntdll, "NtUnmapViewOfSection", c.c_int32, pointer, pointer)
        arguments = [c.POINTER(pointer), c.POINTER(uint), c.POINTER(c.c_int64)]
        if c.sizeof(pointer) == 8:
            arguments.append(c.POINTER(uint))
        else:
            # The selected x86 leaf must prove RET12 before choosing stdcall's
            # three-argument ABI; the historical four-argument declaration is unsafe.
            address = self.resolve(self.base, b"NtInitializeNlsFiles")
            leaf = self.read(address, 16)
            require(leaf[12:15] == b"\xc2\x0c\0", "unmeasured x86 NLS cleanup")
        initialize = bind(self.ntdll, "NtInitializeNlsFiles", c.c_int32, *arguments)
        directory = c.create_unicode_buffer(32768)
        system_directory = bind(
            self.kernel, "GetSystemDirectoryW", uint, c.c_wchar_p, uint
        )
        chars = system_directory(directory, len(directory))
        require(0 < chars < len(directory), "system directory query")
        path = Path(directory.value) / "locale.nls"
        with path.open("rb") as source:
            file_bytes = source.read(MAX_NLS_BYTES + 1)
        require(0 < len(file_bytes) <= MAX_NLS_BYTES, "installed locale.nls bound")
        base, locale = pointer(), uint(0x42424242)
        casing, version = c.c_int64(0x5353535353535353), uint(0x74747474)
        values = [c.byref(base), c.byref(locale), c.byref(casing)]
        if len(arguments) == 4:
            values.append(c.byref(version))
        # A new build could return a pre-existing view. Enumerate allocation
        # identities before the call so cleanup cannot unmap Python's NLS state.
        allocations = set()
        cursor = 0
        for _ in range(65536):
            region = Region()
            got = query(cursor, c.byref(region), c.sizeof(region))
            if got == 0:
                require(c.get_last_error() == 87, "native address-space enumeration")
                break
            require(got == c.sizeof(region), "short native region query")
            if region.allocation:
                allocations.add(region.allocation)
            following = (region.base or 0) + region.size
            require(following > cursor, "non-progressing native region query")
            cursor = following
        else:
            raise RuntimeError("native allocation enumeration exceeds bound")
        status = initialize(*values)
        emit("nls_status", status=f"{status & 0xFFFFFFFF:08x}")
        require(status >= 0 and bool(base.value), "native NLS initialization failed")
        owned = False
        try:
            region = Region()
            require(
                query(base, c.byref(region), c.sizeof(region)) == c.sizeof(region),
                "NLS region query",
            )
            owned = (
                base.value not in allocations
                and region.allocation == base.value
                and region.kind == 0x40000
            )
            require(owned, "NLS result is not a newly owned mapped view")
            require(
                region.base == base.value
                and region.allocation == base.value
                and 0 < region.size <= MAX_NLS_BYTES,
                "bounded native NLS allocation",
            )
            data = self.read(base.value, region.size)
            emit(
                "nls",
                locale=locale.value,
                casing_unchanged=casing.value == 0x5353535353535353,
                version_unchanged=version.value == 0x74747474,
                region_bytes=region.size,
                allocation_protection=region.allocation_protection,
                state=region.state,
                protection=region.protection,
                memory_type=region.kind,
                mapping_sha256=hashlib.sha256(data).hexdigest(),
                file_bytes=len(file_bytes),
                file_sha256=hashlib.sha256(file_bytes).hexdigest(),
                file_prefix_equal=data[: len(file_bytes)] == file_bytes,
                zero_padding=not any(data[len(file_bytes) :]),
            )
        finally:
            if owned:
                closed = unmap(-1, base)
                emit("nls_unmap", status=f"{closed & 0xFFFFFFFF:08x}")
                require(closed >= 0, "native NLS unmap failed")
            else:
                emit("nls_unmap", performed=False, reason="mapping ownership unproven")


def main():
    native = Native()
    version = sys.getwindowsversion()
    emit(
        "host",
        version=[version.major, version.minor, version.build],
        machine=f"{native.machine:04x}",
        pointer_bytes=c.sizeof(c.c_void_p),
        ntdll_image_bytes=native.image_bytes,
    )
    for name in ("NtQuerySystemTime", "ZwQuerySystemTime", "NtInitializeNlsFiles"):
        native.code(name)
    native.clocks()
    native.nls()
    emit("complete", scope="native clock and valid NLS call; no emulator qualification")


if __name__ == "__main__":
    main()
