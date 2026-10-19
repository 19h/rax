from pathlib import Path
import json,ctypes,datetime
s=Path(__file__).parent;d=json.loads((s/'attributes-owner-epoch2-launch.json').read_text());k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[ctypes.c_uint32,ctypes.c_int,ctypes.c_uint32];k.OpenProcess.restype=ctypes.c_void_p
k.CloseHandle.argtypes=[ctypes.c_void_p]
k.GetExitCodeProcess.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_uint32)]
k.GetProcessTimes.argtypes=[ctypes.c_void_p,*([ctypes.POINTER(ctypes.c_uint64)]*4)]
h=k.OpenProcess(0x1000,False,d['child']);assert h
try:
 code=ctypes.c_uint32();assert k.GetExitCodeProcess(h,ctypes.byref(code))
 times=[ctypes.c_uint64() for _ in range(4)];assert k.GetProcessTimes(h,*[ctypes.byref(x)for x in times])
 print(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'pid':d['child'],'exit-or-still-active':code.value,'kernel-seconds':times[2].value/1e7,'user-seconds':times[3].value/1e7}),flush=True)
finally:k.CloseHandle(h)
