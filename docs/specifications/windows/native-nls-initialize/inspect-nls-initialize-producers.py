from pathlib import Path
import datetime,hashlib,json,struct,sys
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
def identity(p):
 data=p.read_bytes();return {'path':str(p),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
rows=[]
for profile,machine in [('arm64',0xaa64),('x64',0x8664),('x86',0x14c),('x86-laa',0x14c)]:
 d=b/('nls-initialize-'+profile);source=d/'native-nls-initialize.cpp';exe=d/('native-nls-initialize-'+profile+'.exe');log=d/('native-nls-initialize-'+profile+'.log')
 src=identity(source)
 if src['sha256']!='b51a47f4bfaf3b322589ca56ec79f0fbd140c57faae18e3d11ad8aeec3d1394b':raise ValueError('actual source')
 data=exe.read_bytes();pe=struct.unpack_from('<I',data,0x3c)[0]
 if data[:2]!=b'MZ' or data[pe:pe+4]!=b'PE\0\0' or struct.unpack_from('<H',data,pe+4)[0]!=machine:raise ValueError('actual machine')
 characteristics=struct.unpack_from('<H',data,pe+22)[0]
 if profile.startswith('x86') and bool(characteristics&32)!=(profile=='x86-laa'):raise ValueError('LAA profile')
 text=log.read_text();last=text.splitlines()[-1]
 if last!='complete cases=79 success=35 failed=44 seh=0 unmapped='+('35' if profile.startswith('x86') else '34'):raise ValueError(last)
 row={'profile':profile,'source':src,'executable':identity(exe),'stdout':identity(log),'machine':machine,'characteristics':characteristics,'terminal':last}
 if identity(s/log.name)['sha256']!=row['stdout']['sha256']:raise ValueError('copied stdout identity')
 rows.append(row)
r={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':list(sys.getwindowsversion()),'ntdll':identity(Path('C:/Windows/System32/ntdll.dll')),'producers':rows}
(s/'native-nls-initialize-v4-identities.json').write_text(json.dumps(r,indent=2)+'\n');print('Independently verified4nativeproducers,316calls,zeroSEH,138unmaps;native sources,PE profiles,executables andstdout')
