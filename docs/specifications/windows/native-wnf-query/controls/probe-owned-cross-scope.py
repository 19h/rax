"""Cross-scope routing only on two probe-owned temporary WNF names."""
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import datetime,hashlib,json
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');out=s/'wnf-owned-cross-scope.json'
if out.exists():raise RuntimeError('cross-scope controls already exist')
n=c.WinDLL('ntdll',use_last_error=True);a=c.WinDLL('advapi32',use_last_error=True);k=c.WinDLL('kernel32',use_last_error=True)
n.NtCreateWnfStateName.argtypes=[c.POINTER(c.c_uint64),w.ULONG,w.ULONG,c.c_ubyte,c.c_void_p,w.ULONG,c.c_void_p];n.NtCreateWnfStateName.restype=w.LONG
n.NtUpdateWnfStateData.argtypes=[c.POINTER(c.c_uint64),c.c_void_p,w.ULONG,c.c_void_p,c.c_void_p,w.ULONG,w.ULONG];n.NtUpdateWnfStateData.restype=w.LONG
n.NtQueryWnfStateData.argtypes=[c.POINTER(c.c_uint64),c.c_void_p,c.c_void_p,c.POINTER(w.ULONG),c.c_void_p,c.POINTER(w.ULONG)];n.NtQueryWnfStateData.restype=w.LONG
n.NtDeleteWnfStateName.argtypes=[c.POINTER(c.c_uint64)];n.NtDeleteWnfStateName.restype=w.LONG
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes=[w.LPCWSTR,w.DWORD,c.POINTER(c.c_void_p),c.POINTER(w.ULONG)];a.ConvertStringSecurityDescriptorToSecurityDescriptorW.restype=w.BOOL
k.LocalFree.argtypes=[c.c_void_p];k.LocalFree.restype=c.c_void_p
sd,size=c.c_void_p(),w.ULONG();owned=[];rows=[];cleanup=[]
result={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'scope':'only newly created probe-owned temporary names; no existing state modified','rows':rows,'cleanup':cleanup}
def save():out.write_text(json.dumps(result,indent=2)+'\n')
if not a.ConvertStringSecurityDescriptorToSecurityDescriptorW('O:SYG:SYD:(A;;0x10003;;;WD)',1,c.byref(sd),c.byref(size)):raise c.WinError(c.get_last_error())
try:
 for scope,explicit in [(1,w.DWORD(1)),(2,c.create_string_buffer(bytes.fromhex('010100000000000513000000'),12))]:
  name=c.c_uint64();status=n.NtCreateWnfStateName(c.byref(name),3,scope,0,None,64,sd)
  rows.append({'op':'create','scope':scope,'name':f'{name.value:016x}','status':status&0xffffffff});save()
  if status<0:raise RuntimeError('private name creation failed')
  owned.append(name)
  value=w.DWORD(0x11223344)
  for label,target in [('current',None),('other',c.byref(explicit))]:
   status=n.NtUpdateWnfStateData(c.byref(name),c.byref(value),4,None,target,0,0)
   rows.append({'op':'update','scope':scope,'target':label,'status':status&0xffffffff});save()
  for label,target in [('current',None),('other',c.byref(explicit))]:
   stamp,length=w.ULONG(0xA5A5A5A5),w.ULONG(64);data=c.create_string_buffer(b'\xA5'*64,64)
   status=n.NtQueryWnfStateData(c.byref(name),None,target,c.byref(stamp),data,c.byref(length))
   rows.append({'op':'query','scope':scope,'target':label,'status':status&0xffffffff,'stamp':stamp.value,'length':length.value,'bytes':data.raw.hex()});save()
finally:
 for name in owned:cleanup.append({'kind':'state','name':f'{name.value:016x}','status':n.NtDeleteWnfStateName(c.byref(name))&0xffffffff});save()
 cleanup.append({'kind':'descriptor','ok':not bool(k.LocalFree(sd))});save()
print(rows,cleanup)
if any(x.get('status',0)!=0 or x.get('ok',True)is not True for x in cleanup):raise RuntimeError('owned cleanup failed')
