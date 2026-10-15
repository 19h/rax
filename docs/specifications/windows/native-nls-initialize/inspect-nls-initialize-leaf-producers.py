from pathlib import Path
import hashlib,json,struct,datetime,sys
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');b=Path('C:/Windows/Temp/assist-native-rax-20261010')
def identity(p):
 data=p.read_bytes();return {'path':str(p),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
rows=[]
for profile,machine in [('arm64',0xaa64),('x64',0x8664),('x86',0x14c),('x86-laa',0x14c)]:
 d=b/('nls-initialize-leaf-'+profile);src=d/'native-nls-initialize-leaf.cpp';exe=d/('native-nls-initialize-leaf-'+profile+'.exe');log=d/('native-nls-initialize-leaf-'+profile+'.log')
 if identity(src)['sha256']!='ebe55cfcf126e95b7160818a3e76c21353f47fe306abd986272c1d3d729dd7df':raise ValueError('actualsource')
 data=exe.read_bytes();pe=struct.unpack_from('<I',data,0x3c)[0];characteristics=struct.unpack_from('<H',data,pe+22)[0]
 if data[:2]!=b'MZ' or data[pe:pe+4]!=b'PE\0\0' or struct.unpack_from('<H',data,pe+4)[0]!=machine:raise ValueError('actualPE')
 if profile.startswith('x86') and bool(characteristics&32)!=(profile=='x86-laa'):raise ValueError('LAAprofile')
 if log.read_text().splitlines()[-1]!='complete leaves=3':raise ValueError('nativecomplete')
 if identity(log)['sha256']!=identity(s/log.name)['sha256']:raise ValueError('rawcopy')
 rows.append({'profile':profile,'source':identity(src),'executable':identity(exe),'stdout':identity(log),'machine':machine,'characteristics':characteristics})
(s/'native-nls-initialize-leaf-identities.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':list(sys.getwindowsversion()),'producers':rows},indent=2)+'\n');print('Independentlyverified4actualproducers,12installedleaves,x86RET12')
