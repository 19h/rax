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

import gzip
k.VirtualProtect.argtypes=[c.c_void_p,c.c_size_t,c.c_uint32,c.POINTER(c.c_uint32)];k.VirtualProtect.restype=c.c_int
def capture(base,size):
 data=bytearray(size);regions=[];at=0
 while at<size:
  r=region(base+at);assert r['base']==base+at and r['allocation']==base and r['kind']==0x1000000 and 0<r['size']<=size-at
  regions.append(dict(offset=at,**r))
  if r['state']==0x1000:
   altered=bool(r['protect']&0x101);old=c.c_uint32()
   if altered:assert k.VirtualProtect(base+at,r['size'],2,c.byref(old))
   try:
    raw=read(base+at,r['size']);assert raw is not None;data[at:at+r['size']]=raw
   finally:
    if altered:
     ignored=c.c_uint32();assert k.VirtualProtect(base+at,r['size'],old.value,c.byref(ignored))
  else:assert r['state']==0x2000
  at+=r['size']
 return data,[{key:r[key]for key in ['offset','size','state','protect']}for r in regions]
def normalized(raw,destination):
 data=bytearray(raw);at=struct.unpack_from('<I',data,60)[0];source=struct.unpack_from('<Q',data,at+48)[0];delta=(destination-source)&0xffffffffffffffff
 rva,n=struct.unpack_from('<II',data,at+24+112+5*8);end=rva+n;kinds={};count=0
 while rva<end:
  page,sz=struct.unpack_from('<II',data,rva);assert sz>=8 and sz%2==0 and rva+sz<=end
  for pos in range(rva+8,rva+sz,2):
   v=struct.unpack_from('<H',data,pos)[0];kind=v>>12;kinds[kind]=kinds.get(kind,0)+1
   if kind==0:continue
   assert kind==10,(kind,rva,pos);off=page+(v&4095);assert off+8<=len(data);value=struct.unpack_from('<Q',data,off)[0];struct.pack_into('<Q',data,off,(value+delta)&0xffffffffffffffff);count+=1
  rva+=sz
 struct.pack_into('<Q',data,at+48,destination)
 return data,kinds,count

class Param(c.Structure):_fields_=[('kind',c.c_uint64),('value',c.c_uint64)]
ex=nt.NtMapViewOfSectionEx
ex.argtypes=[c.c_void_p,c.c_void_p,c.POINTER(c.c_void_p),c.c_void_p,c.POINTER(c.c_size_t),c.c_uint32,c.c_uint32,c.POINTER(Param),c.c_uint32];ex.restype=c.c_long

before=count();out=[];root=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
for name in ['ntdll.dll','kernelbase.dll','kernel32.dll']:
 buf=c.create_unicode_buffer('\\KnownDlls\\'+name);u=US(len(buf.value)*2,(len(buf.value)+1)*2,c.cast(buf,c.c_void_p));oa=OA(48,None,c.pointer(u),0x40,None,None);handle=c.c_void_p();views=[];images=[]
 assert nt.NtOpenSection(c.byref(handle),5,c.byref(oa))>=0
 try:
  for idx,flags in enumerate([0,0x800000,0x800000]):
   base=c.c_void_p();size=c.c_size_t();param=Param(6,0x8664);status=ex(handle,c.c_void_p(-1),c.byref(base),None,c.byref(size),flags,2,c.byref(param),1);assert status>=0;views.append(base)
   raw,regions=capture(base.value,size.value);at=struct.unpack_from('<I',raw,60)[0];preferred=struct.unpack_from('<Q',raw,at+48)[0]
   path=root/(name+'.projected-view'+str(idx)+'.bin.gz');path.write_bytes(gzip.compress(raw,mtime=0));images.append(raw)
   out.append({'name':name,'index':idx,'flags':flags,'base':base.value,'size':size.value,'status':status&0xffffffff,'preferred':preferred,'sha256':hashlib.sha256(raw).hexdigest(),'archive':path.name,'archive-sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'regions':regions,'extension':query(base.value,14,0)})
  preferred=out[-3]['preferred'];one,kinds,n=normalized(images[1],preferred);two,kinds2,n2=normalized(images[2],preferred)
  assert one==two,'two flagged image normalizations differ'
  assert out[-1]['regions']==out[-2]['regions']==out[-3]['regions'],'page policies differ'
  ranges=[];start=None
  for idx,(a,b)in enumerate(zip(images[0],one)):
   if a!=b and start is None:start=idx
   if a==b and start is not None:ranges.append([start,idx]);start=None
  if start is not None:ranges.append([start,len(one)])
  out[-1]['normalization']={'normalized-sha256':hashlib.sha256(one).hexdigest(),'equal':one==two,'kinds':kinds,'fixups':n,'difference-ranges':ranges,'difference-bytes':sum(b-a for a,b in ranges)}
 finally:
  for view in views:assert nt.NtUnmapViewOfSection(c.c_void_p(-1),view)==0
  assert nt.NtClose(handle)==0
after=count();assert before==after
(root/'native-image-extension-projected-normalization.json').write_text(json.dumps({'handles-before':before,'handles-after':after,'rows':out},indent=2)+'\n')
for r in out:
 if 'normalization'in r:print(r['name'],r['normalization']['equal'],r['normalization']['fixups'],'diff bytes',r['normalization']['difference-bytes'],'ranges',len(r['normalization']['difference-ranges']),flush=True)
