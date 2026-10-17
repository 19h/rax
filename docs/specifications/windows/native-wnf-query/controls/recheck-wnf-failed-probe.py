from pathlib import Path
import ctypes as c,json,datetime
n=c.WinDLL('ntdll');q=n.NtQueryWnfStateData;q.argtypes=[c.POINTER(c.c_uint64),c.c_void_p,c.c_void_p,c.POINTER(c.c_ulong),c.c_void_p,c.POINTER(c.c_ulong)];q.restype=c.c_long
name=c.c_uint64(4739561890675187781);stamp=c.c_ulong(0xa5a5a5a5);length=c.c_ulong(64);buf=c.create_string_buffer(64)
status=q(c.byref(name),None,None,c.byref(stamp),buf,c.byref(length))
r={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'name':hex(name.value),'status':status&0xffffffff,'stamp':stamp.value,'length':length.value,'scope':'read-only check of failed-probe temporary name after its creator process exited'}
p=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/wnf-failed-probe-name-recheck.json');p.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
