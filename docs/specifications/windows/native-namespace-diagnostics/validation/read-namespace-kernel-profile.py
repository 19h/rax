from pathlib import Path
import ctypes,datetime,json
from ctypes import wintypes as W
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');f=S/'namespace-kernel-profile.json';assert not f.exists()
class Version(ctypes.Structure):
 _fields_=[('size',W.DWORD),('major',W.DWORD),('minor',W.DWORD),('build',W.DWORD),('platform',W.DWORD),('service_pack',W.WCHAR*128)]
v=Version();v.size=ctypes.sizeof(v);nt=ctypes.WinDLL('ntdll');nt.RtlGetVersion.argtypes=[ctypes.POINTER(Version)];nt.RtlGetVersion.restype=W.LONG;status=nt.RtlGetVersion(ctypes.byref(v));assert status==0
kernel=ctypes.WinDLL('kernel32');kernel.GetNativeSystemInfo.argtypes=[ctypes.c_void_p];kernel.GetNativeSystemInfo.restype=None;info=ctypes.create_string_buffer(128);kernel.GetNativeSystemInfo(info);arch=int.from_bytes(info.raw[:2],'little')
d={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'finished','exit-code':0,'RtlGetVersion-status':status,'major':v.major,'minor':v.minor,'build':v.build,'native-processor-architecture':arch,'scope':'Read-only native kernel/architecture metadata; no guest or host state mutation.'};f.write_text(json.dumps(d,indent=2)+'\n');print(json.dumps(d),flush=True)
