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
out=[]
for name in ['ntdll.dll','kernelbase.dll','kernel32.dll']:
 base=k.GetModuleHandleW(name);assert base
 dos=read(base,64);peoff=struct.unpack_from('<I',dos,60)[0];hdr=read(base+peoff,256);assert hdr[:4]==b'PE\0\0'
 machine=struct.unpack_from('<H',hdr,4)[0];assert machine==0xaa64
 opt=24;size=struct.unpack_from('<I',hdr,opt+56)[0];preferred=struct.unpack_from('<Q',hdr,opt+24)[0];rva,extent=struct.unpack_from('<II',hdr,opt+112+10*8)
 path=c.create_unicode_buffer(32768);assert k.GetModuleFileNameW(base,path,len(path))
 disk=Path(path.value).read_bytes();record={'name':name,'path':path.value,'sha256':hashlib.sha256(disk).hexdigest(),'actual-base':base,'header-preferred-base':preferred,'image-size':size,'machine':machine,'load-config-rva':rva,'load-config-size':extent,'samples':[sample(base+size),sample(base+size+0xc0)],'load-config-pointers':[]}
 cfg=read(base+rva,min(extent,384)) if rva else None
 if cfg:
  for off,label in [(0x70,'GuardCFCheckFunctionPointer'),(0x78,'GuardCFDispatchFunctionPointer')]:
   if off+8<=len(cfg):
    slot=struct.unpack_from('<Q',cfg,off)[0];p=read(slot,8)if slot else None;target=struct.unpack('<Q',p)[0]if p else None;record['load-config-pointers'].append({'field':label,'slot':slot,'target':target,'sample':sample(target)if target else None})
 if name=='kernelbase.dll':record['samples'].append(sample(base+0x306030))
 record['queries']=[query(base,6),query(base,14,0),query(base,14,1)]
 out.append(record)
p=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/native-cfg-dispatch-v2.json');p.write_text(json.dumps({'scope':'read-only loaded system DLL metadata; no state modifications','modules':out},indent=2)+'\n');print(json.dumps(out))
