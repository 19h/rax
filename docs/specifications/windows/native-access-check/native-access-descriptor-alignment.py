"""Controlled ARM64 NtAccessCheck probe on private duplicated token objects."""
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import datetime, hashlib, json, os, platform

S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
O = S / 'native-access-descriptor-alignment.json'
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

descriptor=c.c_void_p(); size=w.DWORD()
check(a.ConvertStringSecurityDescriptorToSecurityDescriptorW('O:SYG:SYD:(A;;0x1;;;WD)',1,c.byref(descriptor),c.byref(size)))
rows=[]
try:
 raw=c.string_at(descriptor,size.value)
 for offset in range(16):
  storage=c.create_string_buffer(size.value+16)
  c.memmove(c.addressof(storage)+offset,raw,len(raw))
  buffer=c.create_string_buffer(b'\xA5'*64,64)
  length,grant,status=w.DWORD(56),w.DWORD(0xA5A5A5A5),w.LONG(-1515870811)
  mapping=Mapping(1,2,4,7)
  c.set_last_error(0x7E11ABCD)
  result=n.NtAccessCheck(c.addressof(storage)+offset,w.HANDLE(-4),1,c.byref(mapping),buffer,c.byref(length),c.byref(grant),c.byref(status))
  rows.append({'offset':offset,'return':result & 0xFFFFFFFF,'returned':length.value,'granted':grant.value,'access-status':status.value & 0xFFFFFFFF,'privileges':buffer.raw.hex(),'last-error':c.get_last_error()})
finally:
 if k.LocalFree(descriptor):raise RuntimeError('descriptor cleanup failed')
result={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'pid':os.getpid(),'machine':platform.machine(),'pointer-bytes':c.sizeof(c.c_void_p),'version':platform.version(),'source-sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'released-descriptors':1,'rows':rows}
O.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print([(r['offset'],hex(r['return'])) for r in rows])
