from pathlib import Path
import json,re,hashlib
B=Path(__file__).resolve().parent
profiles=['arm64','x64','x86','x86-laa'];fields=['kind','role','length','status','output-bytes-cells','return-bytes-cells','output-protect','returned-protect']
canonical=None
for profile in profiles:
 text=(B/f'token-session-id-{profile}.log').read_text()
 rows=[dict(re.findall(r'([\w-]+)=([^\s]*)',x))for x in text.splitlines()if x.startswith('case ')]
 assert len(rows)==1830 and [int(x['ordinal'])for x in rows]==list(range(1830))
 selected=[{k:r[k]for k in fields}for r in rows]
 if canonical is None:canonical=selected
 elif canonical!=selected:raise RuntimeError('native profile semantics differ')
(B/'cases.json').write_text(json.dumps({'class':12,'profiles':profiles,'per-profile':1830,'native-value':0,'cases':canonical},indent=2)+'\n')
items=[]
for name in ['native-token-session-id.cpp','native-token-session-id.ps1','run-token-session-id.py','source-baseline.json','native-identities.json','token-session-id-launch.json','token-session-id-post-run.json','cases.json',*[f'token-session-id-{p}{s}'for p in profiles for s in ['.log','-build.log','-identity.json']]]:
 raw=(B/name).read_bytes();items.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(B/'native-inputs.json').write_text(json.dumps({'files':items},indent=2)+'\n')
print('Prepared exact native session-id corpus',len(canonical),'x',len(profiles))
