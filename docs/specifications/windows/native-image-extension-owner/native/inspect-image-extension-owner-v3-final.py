from pathlib import Path
import hashlib,json
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');rows=[]
for profile in ['arm64','x64','x86','x86-laa']:
 root=b/('image-extension-owner-v3-'+profile);stem='native-image-extension-owner-probe-v3-'+profile
 source=root/'native-image-extension-owner-probe-v3.cpp';exe=root/(stem+'.exe');raw=exe.read_bytes();raw_identity=(s/(stem+'-identity.json')).read_bytes();p=json.loads(raw_identity.decode('utf-16') if raw_identity[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw_identity.decode('utf-8-sig'));assert p['sha256']==hashlib.sha256(raw).hexdigest();assert hashlib.sha256(source.read_bytes()).hexdigest()=='ca9e1ad7e97200e2dd6c793e0d56875b271f36fa32fbfaf147782ae370943092';p['source-sha256']=hashlib.sha256(source.read_bytes()).hexdigest();p['stdout-sha256']=hashlib.sha256((s/(stem+'.log')).read_bytes()).hexdigest();rows.append(p)
(s/'image-extension-owner-v3-final-producer-identities.json').write_text(json.dumps(rows,indent=2)+'\n');print('verified',len(rows),'post-run producer identities')
