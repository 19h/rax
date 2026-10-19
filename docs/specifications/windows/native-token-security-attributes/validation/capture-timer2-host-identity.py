from pathlib import Path
import datetime,hashlib,json,platform,struct,sys
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
v=sys.getwindowsversion()
x={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'machine':platform.machine(),'python':sys.version,'pointer-bytes':struct.calcsize('P'),'windows-version':{'major':v.major,'minor':v.minor,'build':v.build,'platform':v.platform,'service-pack':v.service_pack,'platform-version':list(v.platform_version)},'source-sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'runtime-modules':[],'scope':'Read-only host identity; all four producer ABI profiles share this kernel.'}
for path in ['C:/Windows/System32/ntdll.dll','C:/Windows/SysWOW64/ntdll.dll']:
 p=Path(path);raw=p.read_bytes();x['runtime-modules'].append({'path':path,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(S/'timer2-native-host-identity.json').write_text(json.dumps(x,indent=2)+'\n')
print(json.dumps(x),flush=True)
