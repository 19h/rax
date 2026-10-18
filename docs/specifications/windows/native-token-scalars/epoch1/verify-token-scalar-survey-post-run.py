from pathlib import Path
import hashlib,json,struct
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');rows=[]
for profile in ['arm64','x64','x86','x86-laa']:
 root=b/('token-scalar-survey-'+profile);src=root/'native-token-scalar-survey.cpp';exe=root/'probe.exe';raw=src.read_bytes()
 if hashlib.sha256(raw).hexdigest()!='5a7ef1350ac9244c3970b574de34e1ca1e5953e9954b7d0e1770251b758a642c':raise RuntimeError('source changed '+profile)
 image=exe.read_bytes();pe=struct.unpack_from('<I',image,0x3c)[0];machine=struct.unpack_from('<H',image,pe+4)[0];flags=struct.unpack_from('<H',image,pe+22)[0]
 rows.append({'profile':profile,'source':{'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()},'executable':{'bytes':len(image),'sha256':hashlib.sha256(image).hexdigest(),'machine':machine,'large-address-aware':bool(flags&0x20)}})
(s/'token-scalar-survey-post-run.json').write_text(json.dumps(rows,indent=2)+'\n');print('Verified four post-run scalar query sources and PEs')
