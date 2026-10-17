import ctypes as c,struct,json,hashlib
from pathlib import Path
k=c.WinDLL('kernel32',use_last_error=True)
k.GetModuleHandleW.argtypes=[c.c_wchar_p];k.GetModuleHandleW.restype=c.c_void_p
k.ReadProcessMemory.argtypes=[c.c_void_p,c.c_void_p,c.c_void_p,c.c_size_t,c.POINTER(c.c_size_t)];k.ReadProcessMemory.restype=c.c_int
class M(c.Structure):
 _fields_=[('base',c.c_void_p),('allocation',c.c_void_p),('allocation_protect',c.c_uint32),('partition',c.c_uint32),('size',c.c_size_t),('state',c.c_uint32),('protect',c.c_uint32),('kind',c.c_uint32),('padding',c.c_uint32)]
assert c.sizeof(M)==48
k.VirtualQuery.argtypes=[c.c_void_p,c.POINTER(M),c.c_size_t];k.VirtualQuery.restype=c.c_size_t
k.GetModuleFileNameW.argtypes=[c.c_void_p,c.c_wchar_p,c.c_uint32];k.GetModuleFileNameW.restype=c.c_uint32
def region(a):
 m=M();n=k.VirtualQuery(a,c.byref(m),48)
 if n!=48:return {'query-error':c.get_last_error()}
 return {x:getattr(m,x)for x in ['base','allocation','allocation_protect','size','state','protect','kind']}
def read(a,n):
 r=region(a)
 if r.get('state')!=0x1000 or r['protect']&0x101:return None
 if a+n>r['base']+r['size']:return None
 buf=(c.c_ubyte*n)();got=c.c_size_t()
 if not k.ReadProcessMemory(c.c_void_p(-1),a,buf,n,c.byref(got)) or got.value!=n:return None
 return bytes(buf)
def sample(a):
 raw=read(a,48);v={'address':a,'region':region(a),'bytes':raw.hex()if raw else None}
 if raw:
  ins=struct.unpack_from('<I',raw)[0]
  if ins&0x7c000000==0x14000000:
   imm=ins&0x3ffffff;imm=imm-(1<<26)if imm&(1<<25)else imm;t=a+(imm<<2);v['branch-target']=t;v['target-region']=region(t);x=read(t,48);v['target-bytes']=x.hex()if x else None
 return v
nt=c.WinDLL('ntdll')
nt.NtQueryVirtualMemory.argtypes=[c.c_void_p,c.c_void_p,c.c_uint32,c.c_void_p,c.c_size_t,c.POINTER(c.c_size_t)];nt.NtQueryVirtualMemory.restype=c.c_long
def query(base,kind,extension=None):
 buf=(c.c_ubyte*24)()
 if extension is not None:struct.pack_into('<I',buf,0,extension)
 returned=c.c_size_t(0xcccccccc);status=nt.NtQueryVirtualMemory(c.c_void_p(-1),base,kind,buf,24,c.byref(returned))&0xffffffff
 r={'class':kind,'extension-type':extension,'status':status,'returned':returned.value,'bytes':bytes(buf).hex()}
 if kind==14 and status==0:
  _,flags,rva,size=struct.unpack('<IIQQ',bytes(buf));r.update({'flags':flags,'rva':rva,'size':size})
  if rva and size<=65536:r['sample']=sample(base+rva+0xc0)
 return r

class US(c.Structure):
 _fields_=[('length',c.c_uint16),('maximum',c.c_uint16),('buffer',c.c_void_p)]
class OA(c.Structure):
 _fields_=[('length',c.c_uint32),('root',c.c_void_p),('name',c.POINTER(US)),('attributes',c.c_uint32),('security',c.c_void_p),('qos',c.c_void_p)]
assert c.sizeof(US)==16 and c.sizeof(OA)==48
nt.NtOpenSection.argtypes=[c.POINTER(c.c_void_p),c.c_uint32,c.POINTER(OA)];nt.NtOpenSection.restype=c.c_long
nt.NtQuerySection.argtypes=[c.c_void_p,c.c_uint32,c.c_void_p,c.c_uint32,c.POINTER(c.c_uint32)];nt.NtQuerySection.restype=c.c_long
nt.NtMapViewOfSection.argtypes=[c.c_void_p,c.c_void_p,c.POINTER(c.c_void_p),c.c_size_t,c.c_size_t,c.c_void_p,c.POINTER(c.c_size_t),c.c_uint32,c.c_uint32,c.c_uint32];nt.NtMapViewOfSection.restype=c.c_long
nt.NtUnmapViewOfSection.argtypes=[c.c_void_p,c.c_void_p];nt.NtUnmapViewOfSection.restype=c.c_long
nt.NtClose.argtypes=[c.c_void_p];nt.NtClose.restype=c.c_long
k.GetProcessHandleCount.argtypes=[c.c_void_p,c.POINTER(c.c_uint32)];k.GetProcessHandleCount.restype=c.c_int
def count():
 n=c.c_uint32();assert k.GetProcessHandleCount(c.c_void_p(-1),c.byref(n));return n.value

before=count();out=[]
for name in ['ntdll.dll','kernelbase.dll','kernel32.dll']:
 buf=c.create_unicode_buffer('\\KnownDlls\\'+name);u=US(len(buf.value)*2,(len(buf.value)+1)*2,c.cast(buf,c.c_void_p));oa=OA(48,None,c.pointer(u),0x40,None,None);handle=c.c_void_p();views=[]
 assert nt.NtOpenSection(c.byref(handle),5,c.byref(oa))>=0
 try:
  for flags in [0,0x800000,0x800000]:
   base=c.c_void_p();size=c.c_size_t();status=nt.NtMapViewOfSection(handle,c.c_void_p(-1),c.byref(base),0,0,None,c.byref(size),2,flags,2)
   row={'name':name,'flags':flags,'status':status&0xffffffff,'base':base.value,'size':size.value}
   if status<0:raise RuntimeError(row)
   views.append(base)
   h=read(base.value,4096);at=struct.unpack_from('<I',h,60)[0];row['header-preferred']=struct.unpack_from('<Q',h,at+48)[0]
   row['queries']=[query(base.value,14,t)for t in [0,1]]
   for q in row['queries']:
    if q.get('rva') and q.get('size',0)<=65536:
     raw=read(base.value+q['rva'],q['size']);q['full-bytes']=raw.hex()if raw else None
   row['samples']=[sample(base.value+o) for o in ([0x306000,0x306030,0x306040,0x306050]if name=='kernelbase.dll'else [0x1000])]
   cfg_rva,cfg_size=struct.unpack_from('<II',h,at+24+112+10*8)
   cfg=read(base.value+cfg_rva,min(cfg_size,384))if cfg_rva else None
   row['load-config']=cfg.hex()if cfg else None
   row['guard-pointers']=[]
   if cfg:
    for offset in [0x70,0x78]:
     slot=struct.unpack_from('<Q',cfg,offset)[0];raw=read(slot,8)if slot else None;target=struct.unpack('<Q',raw)[0]if raw else None
     row['guard-pointers'].append({'offset':offset,'slot':slot,'target':target,'sample':sample(target)if target else None})
   out.append(row)
 finally:
  for view in views:assert nt.NtUnmapViewOfSection(c.c_void_p(-1),view)==0
  assert nt.NtClose(handle)==0
after=count();assert before==after
p=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/native-image-extension-flag-comparison.json')
p.write_text(json.dumps({'handles-before':before,'handles-after':after,'rows':out},indent=2)+'\n')
for r in out:
 print(r['name'],hex(r['flags']),hex(r['base']),hex(r['header-preferred']),[(hex(s['address']),s.get('bytes','')[:8],hex(s.get('branch-target',0)))for s in r['samples']],flush=True)
