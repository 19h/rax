"""Native private-handle observation of the exact next loader timer request."""
import ctypes as C
from ctypes import wintypes as W
import datetime
import hashlib
import json
from pathlib import Path

S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
DESTINATION = S / 'native-timer2-set-observed-arm64.json'
assert not DESTINATION.exists()
nt = C.WinDLL('ntdll')
kernel = C.WinDLL('kernel32')
nt.NtCreateTimer2.argtypes = [C.POINTER(W.HANDLE), C.c_void_p, C.c_void_p, W.ULONG, W.ULONG]
nt.NtCreateTimer2.restype = W.LONG
nt.NtSetTimer2.argtypes = [W.HANDLE, C.POINTER(C.c_longlong), C.c_void_p, C.c_void_p]
nt.NtSetTimer2.restype = W.LONG
nt.NtCancelTimer2.argtypes = [W.HANDLE, C.c_void_p]
nt.NtCancelTimer2.restype = W.LONG
nt.NtQueryTimer.argtypes = [W.HANDLE, W.ULONG, C.c_void_p, W.ULONG, C.POINTER(W.ULONG)]
nt.NtQueryTimer.restype = W.LONG
nt.NtWaitForSingleObject.argtypes = [W.HANDLE, W.BYTE, C.POINTER(C.c_longlong)]
nt.NtWaitForSingleObject.restype = W.LONG
nt.NtClose.argtypes = [W.HANDLE]
nt.NtClose.restype = W.LONG
kernel.GetProcessHandleCount.argtypes = [W.HANDLE, C.POINTER(W.DWORD)]
kernel.GetProcessHandleCount.restype = W.BOOL

class Parameters(C.Structure):
    _fields_ = [('Version', W.ULONG), ('Reserved', W.ULONG), ('NoWakeTolerance', C.c_longlong)]


def handles():
    count = W.DWORD()
    assert kernel.GetProcessHandleCount(W.HANDLE(-1), C.byref(count))
    return count.value


def status(value):
    return f'{value & 0xffffffff:08X}'


module = Path('C:/Windows/System32/ntdll.dll')
module_sha = hashlib.sha256(module.read_bytes()).hexdigest()
assert module_sha == '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'scope': 'Only anonymous private timer creation, arming, zero-time polling, cancellation, query and close; no wake/power/security policy changes.',
          'source-sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'module-sha256-before': module_sha, 'handles-before': handles(),
          'attributes': 8, 'desired-access': 0x1f0003, 'observations': []}
handle = W.HANDLE()
created = nt.NtCreateTimer2(C.byref(handle), None, None, 8, 0x1f0003)
result['create-status'] = status(created)
if created >= 0:
    try:
        due = C.c_longlong(-3_000_000_000)
        zero = C.c_longlong(0)
        parameters = Parameters(0, 0, 750_000_000)
        assert C.sizeof(parameters) == 16
        for label, action in [
            ('set-observed', lambda: nt.NtSetTimer2(handle, C.byref(due), None, C.byref(parameters))),
            ('poll-before-cancel', lambda: nt.NtWaitForSingleObject(handle, 0, C.byref(zero))),
            ('cancel', lambda: nt.NtCancelTimer2(handle, None)),
            ('poll-after-cancel', lambda: nt.NtWaitForSingleObject(handle, 0, C.byref(zero))),
        ]:
            result['observations'].append({'operation': label, 'ntstatus': status(action())})
        data = C.create_string_buffer(16)
        length = W.ULONG(0xa5a5a5a5)
        queried = nt.NtQueryTimer(handle, 0, data, len(data), C.byref(length))
        result['query'] = {'ntstatus': status(queried), 'bytes': data.raw.hex(), 'returned-length': length.value}
    finally:
        result['close-status'] = status(nt.NtClose(handle))
result.update({'handles-after': handles(), 'module-sha256-after': hashlib.sha256(module.read_bytes()).hexdigest()})
assert result['handles-before'] == result['handles-after']
assert result['module-sha256-before'] == result['module-sha256-after']
DESTINATION.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result), flush=True)
