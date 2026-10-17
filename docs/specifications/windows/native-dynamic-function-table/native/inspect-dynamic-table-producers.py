from pathlib import Path
import datetime,hashlib,json,struct,sys
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
rows=[]
for kind in ['initial','lifecycle','key','boundary','ranges','ranges-isolated','overlap','overflow','lifetime','zero-overlap','sequence','status-map']:
 for profile in ['arm64','x64','x86','x86-laa']:
  root=b/('dynamic-table-'+kind+'-'+profile);p=root/'probe.exe';raw=p.read_bytes();offset=struct.unpack_from('<I',raw,60)[0]
  if raw[:2]!=b'MZ' or raw[offset:offset+4]!=b'PE\0\0':raise ValueError('PE header')
  machine=struct.unpack_from('<H',raw,offset+4)[0];flags=struct.unpack_from('<H',raw,offset+22)[0]
  if machine!={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile] or bool(flags&0x20)!=(profile!='x86'):raise ValueError('PE profile')
  source=root/('native-dynamic-table-'+kind+'.cpp');src=source.read_bytes()
  outcomes=json.loads((s/('dynamic-table-'+kind+'-outcomes.json')).read_text(encoding='utf-8-sig'))
  logs=[]
  for outcome in outcomes:
   if outcome['profile']!=profile:continue
   data=(s/outcome['log']).read_bytes()
   if outcome['exit-code']!=0:raise ValueError('producer exit '+outcome['log'])
   logs.append({'path':outcome['log'],'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'exit-code':outcome['exit-code']})
  rows.append({'kind':kind,'profile':profile,'source':source.name,'source-bytes':len(src),'source-sha256':hashlib.sha256(src).hexdigest(),'exe':str(p),'exe-bytes':len(raw),'exe-sha256':hashlib.sha256(raw).hexdigest(),'machine':machine,'large-address-aware':bool(flags&0x20),'logs':logs})
result={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'identities':rows,'windows-version':list(sys.getwindowsversion())}
(s/'dynamic-table-final-producer-identities.json').write_text(json.dumps(result,indent=2)+'\n')
print('Post-run source, PE and stdout identities verified:',len(rows))
