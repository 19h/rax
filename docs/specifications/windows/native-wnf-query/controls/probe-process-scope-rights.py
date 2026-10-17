"""Read-only WNF scope validation against handles to this probe process."""
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import hashlib,json,datetime
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');out=s/'wnf-process-scope-rights.json'
if out.exists():raise RuntimeError('result already exists')
k=c.WinDLL('kernel32',use_last_error=True);n=c.WinDLL('ntdll',use_last_error=True)
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
k.GetCurrentProcessId.restype=w.DWORD;k.CloseHandle.argtypes=[w.HANDLE];k.CloseHandle.restype=w.BOOL
n.NtQueryWnfStateData.argtypes=[c.POINTER(c.c_uint64),c.c_void_p,c.c_void_p,c.POINTER(w.ULONG),c.c_void_p,c.POINTER(w.ULONG)];n.NtQueryWnfStateData.restype=w.LONG
rows=[];closed=0
for access in [0,1,0x10,0x400,0x1000,0x1FFFFF]:
 handle=k.OpenProcess(access,False,k.GetCurrentProcessId())
 if not handle:raise c.WinError(c.get_last_error())
 try:
  name=c.c_uint64(0x41911A28A3BC10B5);stamp,length=w.ULONG(0xA5A5A5A5),w.ULONG(0)
  c.set_last_error(0x7E11ABCD)
  status=n.NtQueryWnfStateData(c.byref(name),None,handle,c.byref(stamp),None,c.byref(length))
  rows.append({'access':access,'status':status&0xffffffff,'stamp':stamp.value,'length':length.value,'last-error':c.get_last_error()})
 finally:
  if not k.CloseHandle(handle):raise c.WinError(c.get_last_error())
  closed+=1
out.write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'closed-handles':closed,'rows':rows},indent=2)+'\n')
print(rows)
