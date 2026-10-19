"""Read-only native observation of one fixed loader registry key."""
import ctypes as C
from ctypes import wintypes as W
import datetime
import hashlib
import json
from pathlib import Path
import winreg

S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
PATH = r'\Registry\Machine\System\CurrentControlSet\Control\StateSeparation\RedirectionMap\Keys'
RECORD = S / 'fixed-registry-frontier-native.json'
if RECORD.exists():
    raise RuntimeError('fixed frontier already captured')

class UnicodeString(C.Structure):
    _fields_ = [('Length', W.USHORT), ('MaximumLength', W.USHORT), ('Buffer', C.c_void_p)]

class ObjectAttributes(C.Structure):
    _fields_ = [('Length', W.ULONG), ('RootDirectory', W.HANDLE),
                ('ObjectName', C.POINTER(UnicodeString)), ('Attributes', W.ULONG),
                ('SecurityDescriptor', C.c_void_p), ('SecurityQualityOfService', C.c_void_p)]

nt = C.WinDLL('ntdll')
nt.NtOpenKey.argtypes = [C.POINTER(W.HANDLE), W.ULONG, C.POINTER(ObjectAttributes)]
nt.NtOpenKey.restype = W.LONG
nt.NtClose.argtypes = [W.HANDLE]
nt.NtClose.restype = W.LONG
nt.NtQueryKey.argtypes = [W.HANDLE, W.ULONG, C.c_void_p, W.ULONG, C.POINTER(W.ULONG)]
nt.NtQueryKey.restype = W.LONG

def identity(path):
    data = path.read_bytes()
    return {'path': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}

source = identity(Path(__file__))
module = identity(Path('C:/Windows/System32/ntdll.dll'))
record = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'source-before': source, 'module-before': module, 'path': PATH,
          'scope': 'Read-only fixed system registry key; no creation, modification, or policy changes.',
          'desired-access': 0x20019, 'attributes': 0x40, 'observations': []}
for sample in range(2):
    name = C.create_unicode_buffer(PATH)
    descriptor = UnicodeString(len(PATH.encode('utf-16-le')), C.sizeof(name), C.addressof(name))
    attrs = ObjectAttributes(C.sizeof(ObjectAttributes), None, C.pointer(descriptor), 0x40, None, None)
    handle = W.HANDLE()
    status = nt.NtOpenKey(C.byref(handle), 0x20019, C.byref(attrs)) & 0xffffffff
    row = {'sample': sample, 'nt-open-status': f'{status:08X}', 'handle-nonzero': bool(handle.value)}
    if status == 0:
        try:
            length = W.ULONG()
            first = nt.NtQueryKey(handle, 2, None, 0, C.byref(length)) & 0xffffffff
            row.update(query_size_status=f'{first:08X}', query_required_bytes=length.value)
            if length.value > 1 << 20:
                raise RuntimeError('fixed key metadata exceeds 1 MiB')
            data = C.create_string_buffer(length.value)
            query = nt.NtQueryKey(handle, 2, data, len(data), C.byref(length)) & 0xffffffff
            row.update(query_status=f'{query:08X}', query_bytes=data.raw[:length.value].hex())
        finally:
            row['nt-close-status'] = f'{nt.NtClose(handle) & 0xffffffff:08X}'
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, PATH.removeprefix('\\Registry\\Machine\\'), 0, winreg.KEY_READ) as key:
            children, values, timestamp = winreg.QueryInfoKey(key)
            if children > 4096 or values > 4096:
                raise RuntimeError('fixed root exceeds 4096 entries')
            row['win32'] = {'children': children, 'values': values, 'time': timestamp,
                            'child-names': [winreg.EnumKey(key, i) for i in range(children)],
                            'value-entries': []}
            for i in range(values):
                value_name, value, kind = winreg.EnumValue(key, i)
                if isinstance(value, bytes):
                    value = {'bytes-hex': value.hex()}
                row['win32']['value-entries'].append({'name': value_name, 'kind': kind, 'value': value})
    record['observations'].append(row)
record.update({'source-after': identity(Path(__file__)), 'module-after': identity(Path('C:/Windows/System32/ntdll.dll'))})
assert record['source-before'] == record['source-after']
assert record['module-before'] == record['module-after']
RECORD.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record), flush=True)
