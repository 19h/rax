from pathlib import Path
import datetime, hashlib, json
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
rows=[]
for profile in ['arm64','x64','x86','x86-laa']:
    root=B/('bno-isolation-survey-'+profile)
    for name in ['native-bno-isolation-survey.cpp','probe.exe']:
        p=root/name;raw=p.read_bytes()
        rows.append({'profile':profile,'path':str(p),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
for name in ['native-bno-isolation-child.cpp','child-control.exe']:
    p=B/'bno-isolation-child'/name
    if p.exists():
        raw=p.read_bytes();rows.append({'profile':'child-controller','path':str(p),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(S/'bno-native-post-identities.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':rows},indent=2)+'\n')
print('Post-run identities',len(rows))
