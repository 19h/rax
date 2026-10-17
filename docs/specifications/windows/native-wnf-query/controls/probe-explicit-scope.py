"""Read-only explicit-scope controls for two already selected WNF names."""
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import datetime, hashlib, json

S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
O = S / 'wnf-explicit-scope-controls.json'
if O.exists():
    raise RuntimeError('scope controls already exist')
k = c.WinDLL('kernel32', use_last_error=True)
n = c.WinDLL('ntdll', use_last_error=True)
k.GetCurrentProcessId.restype = w.DWORD
k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]; k.OpenProcess.restype = w.HANDLE
k.CloseHandle.argtypes = [w.HANDLE]; k.CloseHandle.restype = w.BOOL
n.NtQueryWnfStateData.argtypes = [c.POINTER(c.c_uint64), c.c_void_p, c.c_void_p, c.POINTER(w.ULONG), c.c_void_p, c.POINTER(w.ULONG)]
n.NtQueryWnfStateData.restype = w.LONG
pid = w.DWORD(k.GetCurrentProcessId())
handle = k.OpenProcess(0x1000, False, pid.value)
if not handle:
    raise c.WinError(c.get_last_error())
handle_box = w.HANDLE(handle)
scopes = [('none', None), ('current-process', -1), ('current-thread', -2),
          ('real-process', handle), ('invalid', 0xDEAD),
          ('pointer-to-process-handle', c.addressof(handle_box)), ('pointer-to-pid', c.addressof(pid))]
guid = c.create_string_buffer(bytes.fromhex('6364f948d652854e9c02f2c14e17264e'), 16)
rows = []
try:
    for role, value, typed in [('process', 0x41911A28A3BC10B5, False), ('system', 0x0280032EA3BC0875, True)]:
        name = c.c_uint64(value)
        for scope_role, scope in scopes:
            for capacity in [0, 64]:
                stamp, length = w.ULONG(0xA5A5A5A5), w.ULONG(capacity)
                buffer = c.create_string_buffer(b'\xA5' * 64, 64)
                c.set_last_error(0x7E11ABCD)
                status = n.NtQueryWnfStateData(c.byref(name), guid if typed else None, scope,
                                              c.byref(stamp), buffer, c.byref(length))
                rows.append({'name': role, 'scope': scope_role, 'capacity': capacity,
                             'status': status & 0xFFFFFFFF, 'stamp': stamp.value,
                             'length': length.value, 'bytes': buffer.raw.hex(), 'last-error': c.get_last_error()})
finally:
    if not k.CloseHandle(handle):
        raise c.WinError(c.get_last_error())
result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'source-sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'closed-handles': 1, 'rows': rows}
O.write_text(json.dumps(result, indent=2) + '\n')
print([(r['name'], r['scope'], r['capacity'], hex(r['status'])) for r in rows])
