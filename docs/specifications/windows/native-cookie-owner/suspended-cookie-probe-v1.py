"""Read native image cookie state before a disposable child's first instruction."""
from pathlib import Path
import ctypes as c
import hashlib
import json
import struct

SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')


def require(ok, text):
    if not ok:
        raise ValueError(text + ': error=' + str(c.get_last_error()))


def file_cookie(path):
    data = path.read_bytes()
    nt = struct.unpack_from('<I', data, 60)[0]
    optional = nt + 24
    require(struct.unpack_from('<H', data, optional)[0] == 0x20B, 'PE32+ fixture')
    preferred = struct.unpack_from('<Q', data, optional + 24)[0]
    count = struct.unpack_from('<H', data, nt + 6)[0]
    section = optional + struct.unpack_from('<H', data, nt + 20)[0]
    headers = struct.unpack_from('<I', data, optional + 60)[0]
    def raw(rva):
        if rva < headers:
            return rva
        for i in range(count):
            va, size, offset = struct.unpack_from('<III', data, section + 40 * i + 12)
            if va <= rva < va + size:
                return offset + rva - va
        raise ValueError('RVA outside raw sections')
    config = struct.unpack_from('<I', data, optional + 112 + 80)[0]
    cookie_va = struct.unpack_from('<Q', data, raw(config) + 88)[0]
    return {'path': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
            'cookie': struct.unpack_from('<Q', data, raw(cookie_va - preferred))[0]}


def main():
    require(__debug__, 'optimized probe rejected')
    record = SHARE / 'suspended-native-cookie-state.json'
    require(not record.exists(), 'do not replace existing native measurement')
    k = c.WinDLL('kernel32', use_last_error=True)
    class Startup(c.Structure):
        _fields_ = [('size', c.c_ulong), ('reserved', c.c_wchar_p), ('desktop', c.c_wchar_p),
                    ('title', c.c_wchar_p), ('x', c.c_ulong), ('y', c.c_ulong),
                    ('width', c.c_ulong), ('height', c.c_ulong), ('xchars', c.c_ulong),
                    ('ychars', c.c_ulong), ('fill', c.c_ulong), ('flags', c.c_ulong),
                    ('show', c.c_ushort), ('reserved_bytes', c.c_ushort), ('reserved_ptr', c.c_void_p),
                    ('stdin', c.c_void_p), ('stdout', c.c_void_p), ('stderr', c.c_void_p)]
    class Process(c.Structure):
        _fields_ = [('process', c.c_void_p), ('thread', c.c_void_p), ('pid', c.c_ulong), ('tid', c.c_ulong)]
    class Region(c.Structure):
        _fields_ = [('base', c.c_void_p), ('allocation', c.c_void_p), ('allocation_protect', c.c_ulong),
                    ('partition', c.c_ushort), ('size', c.c_size_t), ('state', c.c_ulong),
                    ('protect', c.c_ulong), ('kind', c.c_ulong)]
    k.CreateProcessW.argtypes = [c.c_wchar_p, c.c_wchar_p, c.c_void_p, c.c_void_p, c.c_int,
                                c.c_ulong, c.c_void_p, c.c_wchar_p, c.POINTER(Startup), c.POINTER(Process)]
    k.VirtualQueryEx.argtypes = [c.c_void_p, c.c_void_p, c.POINTER(Region), c.c_size_t]
    k.VirtualQueryEx.restype = c.c_size_t
    k.K32GetMappedFileNameW.argtypes = [c.c_void_p, c.c_void_p, c.c_wchar_p, c.c_ulong]
    k.ReadProcessMemory.argtypes = [c.c_void_p, c.c_void_p, c.c_void_p, c.c_size_t, c.POINTER(c.c_size_t)]
    k.TerminateProcess.argtypes = [c.c_void_p, c.c_uint]
    k.WaitForSingleObject.argtypes = [c.c_void_p, c.c_ulong]
    k.CloseHandle.argtypes = [c.c_void_p]
    exe = BASE / 'cookie-owner-arm64/native-cookie-owner-arm64.exe'
    startup, process = Startup(), Process()
    startup.size = c.sizeof(startup)
    command = c.create_unicode_buffer('"' + str(exe) + '" --child unused-owned-output.txt')
    require(k.CreateProcessW(str(exe), command, None, None, False, 4 | 8, None, str(BASE),
                             c.byref(startup), c.byref(process)), 'create suspended owned child')
    row = {'pid': process.pid, 'first-thread-resumed': False, 'modules': []}
    def read(address, size):
        buffer = c.create_string_buffer(size)
        got = c.c_size_t()
        require(k.ReadProcessMemory(process.process, address, buffer, size, c.byref(got)) and got.value == size,
                'bounded child read')
        return buffer.raw
    try:
        seen = set()
        address = 0
        for _ in range(4096):
            region = Region()
            if not k.VirtualQueryEx(process.process, address, c.byref(region), c.sizeof(region)):
                break
            if region.kind == 0x1000000 and region.allocation not in seen:
                seen.add(region.allocation)
                name = c.create_unicode_buffer(32768)
                require(k.K32GetMappedFileNameW(process.process, region.base, name, len(name)), 'mapped image path')
                if name.value.lower().endswith(('ntdll.dll', 'native-cookie-owner-arm64.exe')):
                    base = region.allocation
                    nt = struct.unpack('<I', read(base + 60, 4))[0]
                    optional = base + nt + 24
                    config = struct.unpack('<I', read(optional + 112 + 80, 4))[0]
                    cookie = struct.unpack('<Q', read(base + config + 88, 8))[0]
                    value = struct.unpack('<Q', read(cookie, 8))[0]
                    disk = Path('C:/Windows/System32/ntdll.dll') if name.value.lower().endswith('ntdll.dll') else exe
                    row['modules'].append({'mapped-path': name.value, 'base': base, 'cookie-va': cookie,
                                           'suspended-cookie': value, 'file': file_cookie(disk)})
            next_address = region.base + region.size
            require(next_address > address, 'strict region progress')
            address = next_address
        require(len(row['modules']) == 2, 'actual executable and NTDLL image readings')
    finally:
        require(k.TerminateProcess(process.process, 0xE00000FE), 'terminate owned suspended child')
        require(k.WaitForSingleObject(process.process, 30000) == 0, 'reap owned suspended child')
        require(k.CloseHandle(process.thread) and k.CloseHandle(process.process), 'close both owned handles')
    row['cleanup'] = 'pass'
    record.write_text(json.dumps(row, indent=2) + '\n')
    print(json.dumps(row, indent=2))


if __name__ == '__main__':
    main()
