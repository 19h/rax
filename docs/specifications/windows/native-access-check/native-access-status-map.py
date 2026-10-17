from pathlib import Path
import ctypes as c,json,hashlib,datetime,platform
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');p=s/'native-access-status-map.json'
if p.exists():raise RuntimeError('mapping capture already exists')
n=c.WinDLL('ntdll',use_last_error=True);n.RtlNtStatusToDosError.argtypes=[c.c_long];n.RtlNtStatusToDosError.restype=c.c_ulong
rows=[]
for value in [0xc000005c,0xc0000079,0xc00000e6]:
 c.set_last_error(0x7e11abcd);result=n.RtlNtStatusToDosError(c.c_long(value));rows.append({'status':value,'error':result,'last-error':c.get_last_error()})
r={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'machine':platform.machine(),'source-sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'rows':rows}
p.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
