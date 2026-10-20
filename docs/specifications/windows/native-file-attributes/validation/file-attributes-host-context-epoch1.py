from pathlib import Path
import ctypes,datetime,hashlib,json,platform
class Version(ctypes.Structure):
 _fields_=[('size',ctypes.c_uint32),('major',ctypes.c_uint32),('minor',ctypes.c_uint32),('build',ctypes.c_uint32),('platform',ctypes.c_uint32),('service_pack',ctypes.c_wchar*128),('service_major',ctypes.c_uint16),('service_minor',ctypes.c_uint16),('suite',ctypes.c_uint16),('product',ctypes.c_uint8),('reserved',ctypes.c_uint8)]
assert ctypes.sizeof(Version)==284
v=Version();v.size=ctypes.sizeof(v);api=ctypes.WinDLL('ntdll').RtlGetVersion;api.argtypes=[ctypes.POINTER(Version)];api.restype=ctypes.c_int32;assert api(ctypes.byref(v))==0
source=Path(__file__);raw=source.read_bytes();result={'state':'finished','exit-code':0,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':{'major':v.major,'minor':v.minor,'build':v.build},'native-python-machine':platform.machine(),'pointer-bytes':ctypes.sizeof(ctypes.c_void_p),'source':{'path':source.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()},'scope':'Read-only live kernel context. Four producer ABI views share this one ARM64 kernel; no independent x64 kernel is claimed.'}
assert result['kernel']=={'major':10,'minor':0,'build':29683};assert result['native-python-machine'].lower()=='arm64'
path=source.with_suffix('.json');assert not path.exists();path.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
