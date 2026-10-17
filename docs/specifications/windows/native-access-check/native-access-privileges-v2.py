"""Controlled ARM64 NtAccessCheck probe on private duplicated token objects."""
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import datetime, hashlib, json, os, platform

S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
O = S / 'native-access-privileges-v2.json'
if O.exists():
    raise RuntimeError('private-token privilege probe already exists')
k = c.WinDLL('kernel32', use_last_error=True)
a = c.WinDLL('advapi32', use_last_error=True)
n = c.WinDLL('ntdll', use_last_error=True)

class Luid(c.Structure):
    _fields_ = [('low', w.DWORD), ('high', w.LONG)]

class Privilege(c.Structure):
    _fields_ = [('luid', Luid), ('attributes', w.DWORD)]

class TokenPrivileges(c.Structure):
    _fields_ = [('count', w.DWORD), ('privilege', Privilege)]

class Mapping(c.Structure):
    _fields_ = [('read', w.DWORD), ('write', w.DWORD), ('execute', w.DWORD), ('all', w.DWORD)]

if (c.sizeof(Luid), c.sizeof(Privilege), c.sizeof(TokenPrivileges), c.sizeof(Mapping)) != (8, 12, 16, 16):
    raise RuntimeError('unexpected Windows ABI')
k.GetCurrentProcess.restype = w.HANDLE
k.CloseHandle.argtypes = [w.HANDLE]; k.CloseHandle.restype = w.BOOL
k.LocalFree.argtypes = [c.c_void_p]; k.LocalFree.restype = c.c_void_p
a.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, c.POINTER(w.HANDLE)]; a.OpenProcessToken.restype = w.BOOL
a.DuplicateTokenEx.argtypes = [w.HANDLE, w.DWORD, c.c_void_p, c.c_int, c.c_int, c.POINTER(w.HANDLE)]
a.DuplicateTokenEx.restype = w.BOOL
a.AdjustTokenPrivileges.argtypes = [w.HANDLE, w.BOOL, c.c_void_p, w.DWORD, c.c_void_p, c.c_void_p]
a.AdjustTokenPrivileges.restype = w.BOOL
a.LookupPrivilegeValueW.argtypes = [w.LPCWSTR, w.LPCWSTR, c.POINTER(Luid)]; a.LookupPrivilegeValueW.restype = w.BOOL
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = [w.LPCWSTR, w.DWORD, c.POINTER(c.c_void_p), c.POINTER(w.DWORD)]
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.restype = w.BOOL
n.NtAccessCheck.argtypes = [c.c_void_p, w.HANDLE, w.DWORD, c.POINTER(Mapping), c.c_void_p,
                          c.POINTER(w.DWORD), c.POINTER(w.DWORD), c.POINTER(w.LONG)]
n.NtAccessCheck.restype = w.LONG

def check(ok):
    if not ok:
        raise c.WinError(c.get_last_error())

handles, descriptors, rows, configurations = [], [], [], []
try:
    primary = w.HANDLE()
    check(a.OpenProcessToken(k.GetCurrentProcess(), 0xA, c.byref(primary)))
    handles.append(primary)
    for label, enabled in [('none', []), ('security', ['SeSecurityPrivilege']),
                           ('take-ownership', ['SeTakeOwnershipPrivilege']),
                           ('both', ['SeSecurityPrivilege', 'SeTakeOwnershipPrivilege'])]:
        token = w.HANDLE()
        # This new impersonation token alone is changed. No thread adopts it.
        check(a.DuplicateTokenEx(primary, 0x28, None, 2, 2, c.byref(token)))
        handles.append(token)
        check(a.AdjustTokenPrivileges(token, True, None, 0, None, None))
        luids = []
        for name in enabled:
            value = TokenPrivileges(); value.count = 1; value.privilege.attributes = 2
            check(a.LookupPrivilegeValueW(None, name, c.byref(value.privilege.luid)))
            c.set_last_error(0)
            check(a.AdjustTokenPrivileges(token, False, c.byref(value), 0, None, None))
            if c.get_last_error() != 0:
                raise c.WinError(c.get_last_error())
            luids.append({'name': name, 'low': value.privilege.luid.low, 'high': value.privilege.luid.high})
        configurations.append({'label': label, 'enabled': luids})
        for descriptor_name, sddl in [
                ('owned-empty', 'O:SYG:SYD:'),
                ('world-empty', 'O:WDG:SYD:'),
                ('foreign-empty', 'O:S-1-5-32-1604681682-535129537-3273749797-3666938095-336295784-2177615760-2743807136-2867270584G:SYD:'),
                ('world-null', 'O:WDG:SYD:NO_ACCESS_CONTROL')]:
            descriptor = c.c_void_p(); size = w.DWORD()
            check(a.ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, 1, c.byref(descriptor), c.byref(size)))
            descriptors.append(descriptor)
            for desired in [0, 1, 0x20000, 0x40000, 0x80000, 0x1000000,
                            0x1080000, 0x2000000, 0x3000000]:
                for capacity in [0, 8, 19, 20, 31, 32, 44, 56, 64]:
                    buffer = c.create_string_buffer(b'\xA5' * 64, 64)
                    length, grant, status = w.DWORD(capacity), w.DWORD(0xA5A5A5A5), w.LONG(-1515870811)
                    mapping = Mapping(1, 2, 4, 7)
                    c.set_last_error(0x7E11ABCD)
                    result = n.NtAccessCheck(descriptor, token, desired, c.byref(mapping), buffer,
                                             c.byref(length), c.byref(grant), c.byref(status))
                    error = c.get_last_error()
                    if error != 0x7E11ABCD:
                        raise RuntimeError('NtAccessCheck changed last error')
                    rows.append({'ordinal': len(rows), 'token': label, 'descriptor': descriptor_name,
                                 'desired': desired, 'capacity': capacity, 'return': result & 0xFFFFFFFF,
                                 'returned': length.value, 'granted': grant.value,
                                 'access-status': status.value & 0xFFFFFFFF, 'privileges': buffer.raw.hex(),
                                 'last-error': error})
finally:
    for descriptor in descriptors:
        if k.LocalFree(descriptor):
            raise RuntimeError('descriptor cleanup failed')
    for handle in reversed(handles):
        check(k.CloseHandle(handle))
result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'pid': os.getpid(),
          'machine': platform.machine(), 'version': platform.version(), 'pointer-bytes': c.sizeof(c.c_void_p),
          'source-sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'closed-handles': len(handles), 'released-descriptors': len(descriptors),
          'scope': 'only private duplicated impersonation tokens adjusted; no thread token or original primary token changed',
          'configurations': configurations, 'rows': rows}
if len(rows) != 1296 or len(handles) != 5 or len(descriptors) != 16:
    raise RuntimeError('incomplete controlled matrix')
O.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print('completed', len(rows), 'native ARM64 calls;', len(handles), 'closed handles;', len(descriptors), 'released descriptors')
