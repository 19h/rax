import pathlib,ctypes,json,hashlib,platform,sys,datetime
share=pathlib.Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
def identity(path):
 data=path.read_bytes();return {'path':str(path),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
function=ctypes.WinDLL('ntdll').RtlNtStatusToDosError
function.argtypes=[ctypes.c_int32];function.restype=ctypes.c_uint32
value=function(ctypes.c_int32(0xC0000078))
record={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'machine':platform.machine(),'kernel':list(sys.getwindowsversion()),'status':'C0000078','win32-error':value,'source':identity(pathlib.Path(__file__)),'python':identity(pathlib.Path(sys.executable)),'ntdll':identity(pathlib.Path('C:/Windows/System32/ntdll.dll'))}
(share/'native-secure-connect-status.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
if value != 1337:raise ValueError('unexpected actual mapping')
