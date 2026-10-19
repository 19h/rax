from pathlib import Path
import ctypes,json,datetime
s=Path(__file__).parent;record=s/'attributes-all-targets-resource-audit.json';assert not record.exists()
d=json.loads((s/'attributes-native-epoch2-gates.json').read_text());k=ctypes.WinDLL('kernel32',use_last_error=True)
class Entry(ctypes.Structure):
 _fields_=[('size',ctypes.c_uint32),('usage',ctypes.c_uint32),('pid',ctypes.c_uint32),('heap',ctypes.c_void_p),('module',ctypes.c_uint32),('threads',ctypes.c_uint32),('parent',ctypes.c_uint32),('priority',ctypes.c_int32),('flags',ctypes.c_uint32),('name',ctypes.c_wchar*260)]
k.CreateToolhelp32Snapshot.argtypes=[ctypes.c_uint32,ctypes.c_uint32];k.CreateToolhelp32Snapshot.restype=ctypes.c_void_p
for name in ['Process32FirstW','Process32NextW']:getattr(k,name).argtypes=[ctypes.c_void_p,ctypes.POINTER(Entry)]
k.CloseHandle.argtypes=[ctypes.c_void_p];k.OpenProcess.argtypes=[ctypes.c_uint32,ctypes.c_int,ctypes.c_uint32];k.OpenProcess.restype=ctypes.c_void_p;k.GetProcessTimes.argtypes=[ctypes.c_void_p,*([ctypes.POINTER(ctypes.c_uint64)]*4)]
h=k.CreateToolhelp32Snapshot(2,0);assert h and h!=ctypes.c_void_p(-1).value
processes=[];e=Entry();e.size=ctypes.sizeof(e)
try:
 more=k.Process32FirstW(h,ctypes.byref(e))
 while more:
  processes.append({'pid':e.pid,'parent':e.parent,'name':e.name,'threads':e.threads});more=k.Process32NextW(h,ctypes.byref(e))
finally:k.CloseHandle(h)
selected={d['child-pid']}
for _ in range(5):selected.update(x['pid']for x in processes if x['parent']in selected)
rows=[]
for row in processes:
 if row['pid']not in selected:continue
 h=k.OpenProcess(0x1000,False,row['pid'])
 if h:
  try:
   t=[ctypes.c_uint64()for _ in range(4)]
   if k.GetProcessTimes(h,*[ctypes.byref(x)for x in t]):row.update(**{'kernel-seconds':t[2].value/1e7,'user-seconds':t[3].value/1e7})
  finally:k.CloseHandle(h)
 rows.append(row)
class Memory(ctypes.Structure):
 _fields_=[('length',ctypes.c_uint32),('load',ctypes.c_uint32),*[(x,ctypes.c_uint64) for x in ['physical-total','physical-available','commit-limit','commit-available','virtual-total','virtual-available','extended-available']]]
mem=Memory();mem.length=ctypes.sizeof(mem);assert k.GlobalMemoryStatusEx(ctypes.byref(mem))
resource={x:getattr(mem,x)for x,_ in mem._fields_}
class Counters(ctypes.Structure):
 _fields_=[('size',ctypes.c_uint32),('faults',ctypes.c_uint32),*[(x,ctypes.c_size_t)for x in ['peak-working','working','peak-paged','paged','peak-nonpaged','nonpaged','commit','peak-commit','private']]]
api=ctypes.WinDLL('psapi');api.GetProcessMemoryInfo.argtypes=[ctypes.c_void_p,ctypes.POINTER(Counters),ctypes.c_uint32]
for row in rows:
 h=k.OpenProcess(0x1010,False,row['pid'])
 if h:
  try:
   values=Counters();values.size=ctypes.sizeof(values)
   if api.GetProcessMemoryInfo(h,ctypes.byref(values),values.size):row['memory-bytes']={x:getattr(values,x)for x in ['working','private','peak-working','peak-commit']}
  finally:k.CloseHandle(h)
r={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'stage':d['current-step'],'memory':resource,'processes':rows,'scope':'Read-only Toolhelp32/query-limited process audit; no process control or priority changes.'};record.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r),flush=True)
