import ctypes as c,json
from pathlib import Path
n=c.c_uint64(4739561890666743877);exists=c.c_uint32(0xcccccccc);nt=c.WinDLL('ntdll');f=nt.NtQueryWnfStateNameInformation
f.argtypes=[c.POINTER(c.c_uint64),c.c_uint32,c.c_void_p,c.c_void_p,c.c_uint32];f.restype=c.c_long
status=f(c.byref(n),0,None,c.byref(exists),4)&0xffffffff
result={'name':hex(n.value),'query-status':status,'exists':exists.value,'scope':'read-only recheck of the probe-owned name after its creator process exited'}
Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/system-wnf-deny-cleanup-recheck.json').write_text(json.dumps(result,indent=2)+'\n');print(result)
