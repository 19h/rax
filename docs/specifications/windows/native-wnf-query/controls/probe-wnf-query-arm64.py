"""Read the captured system state and test only owned temporary-state creation."""
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import datetime, hashlib, json, platform

S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
O = S / 'wnf-query-arm64-controls.json'
if O.exists():
    raise RuntimeError('controls already recorded')
n = c.WinDLL('ntdll', use_last_error=True)
a = c.WinDLL('advapi32', use_last_error=True)
k = c.WinDLL('kernel32', use_last_error=True)
n.NtQueryWnfStateData.argtypes = [c.POINTER(c.c_uint64), c.c_void_p, c.c_void_p, c.POINTER(w.ULONG), c.c_void_p, c.POINTER(w.ULONG)]
n.NtQueryWnfStateData.restype = w.LONG
n.NtCreateWnfStateName.argtypes = [c.POINTER(c.c_uint64), w.ULONG, w.ULONG, c.c_ubyte, c.c_void_p, w.ULONG, c.c_void_p]
n.NtCreateWnfStateName.restype = w.LONG
n.NtDeleteWnfStateName.argtypes = [c.POINTER(c.c_uint64)]
n.NtDeleteWnfStateName.restype = w.LONG
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = [w.LPCWSTR, w.DWORD, c.POINTER(c.c_void_p), c.POINTER(w.ULONG)]
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.restype = w.BOOL
k.LocalFree.argtypes = [c.c_void_p]; k.LocalFree.restype = c.c_void_p
name = c.c_uint64(0x0280032EA3BC0875)
guid = c.create_string_buffer(bytes.fromhex('6364f948d652854e9c02f2c14e17264e'), 16)
queries = []
for typed in [False, True]:
    for attempt in range(3):
        stamp, length = w.ULONG(0xA5A5A5A5), w.ULONG(4096)
        buffer = c.create_string_buffer(b'\xA5' * 4096, 4096)
        c.set_last_error(0x7E11ABCD)
        status = n.NtQueryWnfStateData(c.byref(name), guid if typed else None, None,
                                      c.byref(stamp), buffer, c.byref(length))
        queries.append({'typed': typed, 'attempt': attempt, 'status': status & 0xFFFFFFFF,
                        'stamp': stamp.value, 'bytes': length.value, 'last-error': c.get_last_error(),
                        'data': buffer.raw[:min(length.value, 4096)].hex()})
descriptor, length = c.c_void_p(), w.ULONG()
if not a.ConvertStringSecurityDescriptorToSecurityDescriptorW('O:SYG:SYD:(A;;0x3;;;WD)', 1, c.byref(descriptor), c.byref(length)):
    raise c.WinError(c.get_last_error())
creations = []
try:
    for scope in range(6):
        for maximum in [0, 1, 4, 16, 64, 4096]:
            for use_descriptor in [False, True]:
                created = c.c_uint64()
                status = n.NtCreateWnfStateName(c.byref(created), 3, scope, 0, None, maximum,
                                               descriptor if use_descriptor else None)
                row = {'scope': scope, 'maximum': maximum, 'descriptor': use_descriptor,
                       'status': status & 0xFFFFFFFF, 'name': f'{created.value:016x}'}
                if status >= 0:
                    deleted = n.NtDeleteWnfStateName(c.byref(created))
                    row['delete-status'] = deleted & 0xFFFFFFFF
                    if deleted < 0:
                        raise RuntimeError('owned temporary state cleanup failed')
                creations.append(row)
finally:
    if k.LocalFree(descriptor):
        raise RuntimeError('descriptor cleanup failed')
result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'machine': platform.machine(),
          'source-sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'queries': queries, 'temporary-creations': creations, 'released-descriptors': 1}
O.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'queries': queries, 'create-successes': sum(r['status'] == 0 for r in creations)}))
