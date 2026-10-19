from pathlib import Path
import ctypes,json,datetime
s=Path(__file__).parent;d=json.loads((s/'attributes-native-epoch2-gates.json').read_text());k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[ctypes.c_uint32,ctypes.c_int,ctypes.c_uint32];k.OpenProcess.restype=ctypes.c_void_p;k.CloseHandle.argtypes=[ctypes.c_void_p];k.GetProcessTimes.argtypes=[ctypes.c_void_p,*([ctypes.POINTER(ctypes.c_uint64)]*4)]
h=k.OpenProcess(0x1000,False,d['child-pid']);assert h
try:
 t=[ctypes.c_uint64() for _ in range(4)];assert k.GetProcessTimes(h,*[ctypes.byref(x)for x in t]);print(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'pid':d['child-pid'],'kernel-seconds':t[2].value/1e7,'user-seconds':t[3].value/1e7}),flush=True)
finally:k.CloseHandle(h)
