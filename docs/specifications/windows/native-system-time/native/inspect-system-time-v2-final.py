from pathlib import Path
import hashlib,json,struct
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
expected='7459a8e1a34e9b8a4144a9587af76918f46c23cd2bab926d9966b07e6eda1544'
rows=[]
for profile,machine,laa in [('arm64',0xaa64,True),('x64',0x8664,True),('x86',0x14c,False),('x86-laa',0x14c,True)]:
 stem='native-system-time-v2-probe-'+profile
 raw=(S/(stem+'-identity.json')).read_bytes();before=json.loads(raw.decode('utf-16')if raw[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw.decode('utf-8-sig'))
 exe=Path(before['path']);data=exe.read_bytes();at=struct.unpack_from('<I',data,0x3c)[0]
 actual={'profile':profile,'path':str(exe),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'machine':struct.unpack_from('<H',data,at+4)[0],'large-address-aware':bool(struct.unpack_from('<H',data,at+22)[0]&0x20),'source-sha256':hashlib.sha256((exe.parent/'native-system-time-v2-probe.cpp').read_bytes()).hexdigest(),'stdout-sha256':hashlib.sha256((S/(stem+'.log')).read_bytes()).hexdigest()}
 for key in ['bytes','sha256','machine','large-address-aware']:
  if actual[key]!=before[key]:raise ValueError('post-run producer drift '+key)
 if actual['machine']!=machine or actual['large-address-aware']!=laa or actual['source-sha256']!=expected:raise ValueError('producer target/source mismatch')
 rows.append(actual)
(S/'system-time-v2-final-producer-identities.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps({'verified-profiles':len(rows),'source-sha256':expected}))
