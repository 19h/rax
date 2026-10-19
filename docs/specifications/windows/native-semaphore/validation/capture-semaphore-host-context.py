from pathlib import Path
import ctypes
from ctypes import wintypes as w
import datetime
import hashlib
import json
import platform


class Version(ctypes.Structure):
    _fields_ = [('size', w.DWORD), ('major', w.DWORD), ('minor', w.DWORD),
                ('build', w.DWORD), ('platform', w.DWORD), ('service_pack', w.WCHAR * 128)]


class Luid(ctypes.Structure):
    _fields_ = [('low', w.DWORD), ('high', w.LONG)]


S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
destination = S / 'native-semaphore-host-context.json'
assert not destination.exists()
version = Version()
version.size = ctypes.sizeof(version)
ntdll = ctypes.WinDLL('ntdll')
ntdll.RtlGetVersion.argtypes = [ctypes.POINTER(Version)]
ntdll.RtlGetVersion.restype = w.LONG
status = ntdll.RtlGetVersion(ctypes.byref(version))
assert status == 0
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
advapi = ctypes.WinDLL('advapi32', use_last_error=True)
advapi.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, ctypes.POINTER(w.HANDLE)]
advapi.GetTokenInformation.argtypes = [w.HANDLE, w.DWORD, ctypes.c_void_p, w.DWORD, ctypes.POINTER(w.DWORD)]
advapi.LookupPrivilegeNameW.argtypes = [w.LPCWSTR, ctypes.POINTER(Luid), w.LPWSTR, ctypes.POINTER(w.DWORD)]
kernel.CloseHandle.argtypes = [w.HANDLE]
token = w.HANDLE()
assert advapi.OpenProcessToken(w.HANDLE(-1), 8, ctypes.byref(token))
try:
    size = w.DWORD()
    advapi.GetTokenInformation(token, 3, None, 0, ctypes.byref(size))
    assert size.value >= 4
    buffer = ctypes.create_string_buffer(size.value)
    assert advapi.GetTokenInformation(token, 3, buffer, size, ctypes.byref(size))
    count = int.from_bytes(buffer.raw[:4], 'little')
    assert size.value >= 4 + count * 12
    privileges = []
    for index in range(count):
        at = 4 + 12 * index
        luid = Luid.from_buffer_copy(buffer.raw[at:at + 8])
        attributes = int.from_bytes(buffer.raw[at + 8:at + 12], 'little')
        text = ctypes.create_unicode_buffer(256)
        capacity = w.DWORD(256)
        assert advapi.LookupPrivilegeNameW(None, ctypes.byref(luid), text, ctypes.byref(capacity))
        privileges.append({'name': text.value, 'attributes': f'0x{attributes:08X}',
                           'enabled': bool(attributes & 2)})
finally:
    assert kernel.CloseHandle(token)
record = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'kernel': {'status': status, 'major': version.major, 'minor': version.minor, 'build': version.build},
          'native-python-machine': platform.machine(), 'pointer-bytes': ctypes.sizeof(ctypes.c_void_p),
          'current-process-token-privileges': privileges,
          'source-sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'scope': 'Read-only current-process token and kernel-version query; no privilege or Defender policy changes.'}
destination.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
