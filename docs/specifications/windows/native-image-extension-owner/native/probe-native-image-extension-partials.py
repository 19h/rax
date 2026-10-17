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
 bmod=k.GetModuleHandleW(name);dos=read(bmod,64);off=struct.unpack_from('<I',dos,60)[0];header=read(bmod+off,128);image_size=struct.unpack_from('<I',header,24+56)[0]
 buf=c.create_unicode_buffer('\\KnownDlls\\'+name);u=US(len(buf.value)*2,(len(buf.value)+1)*2,c.cast(buf,c.c_void_p));oa=OA(48,None,c.pointer(u),0x40,None,None);h=c.c_void_p();assert nt.NtOpenSection(c.byref(h),5,c.byref(oa))>=0
 try:
  for flags in [0,0x800000]:
   for requested in [0,4096,65536,image_size-4096,image_size,image_size+4096]:
    base=c.c_void_p();size=c.c_size_t(requested);status=nt.NtMapViewOfSection(h,c.c_void_p(-1),c.byref(base),0,0,None,c.byref(size),2,flags,2)
    r={'name':name,'image-size':image_size,'flags':flags,'requested':requested,'status':status&0xffffffff,'base':base.value,'returned-size':size.value}
    try:
     if status>=0:
      r['image-information']=query(base.value,6);r['extension']=query(base.value,14,0)
      r['regions']={label:region(address)for label,address in [('first',base.value),('image-last',base.value+image_size-1),('extension',base.value+image_size),('extension-last',base.value+image_size+4095)]}
      ex=r['extension'];r['tail-ret']=read(base.value+ex['rva']+0xc0,4).hex()if ex.get('rva')and read(base.value+ex['rva']+0xc0,4)else None
    finally:
     if status>=0:r['unmap-status']=nt.NtUnmapViewOfSection(c.c_void_p(-1),base)&0xffffffff
    out.append(r)
 finally:assert nt.NtClose(h)==0
after=count();assert before==after
Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/native-image-extension-partials.json').write_text(json.dumps({'handles-before':before,'handles-after':after,'rows':out},indent=2)+'\n')
for r in out:print(r['name'],hex(r['flags']),hex(r['requested']),hex(r['status']),hex(r['returned-size']),r.get('extension',{}).get('rva'),r.get('regions',{}).get('extension'),r.get('unmap-status'))
