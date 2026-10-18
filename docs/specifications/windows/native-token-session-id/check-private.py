"""Verify isolated nonzero SessionId observations; no guest setter is implied."""
from pathlib import Path
import hashlib,json,re
D=Path(__file__).resolve().parent
PROFILES=['arm64','x64','x86','x86-laa']
def load(name):
 raw=(D/name).read_bytes()
 return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw.decode('utf-8-sig'))
def need(ok,why):
 if not ok:raise ValueError(why)
need(__debug__,'optimized private checker rejected')
for row in load('private-inputs.json')['files']:
 raw=(D/row['path']).read_bytes();need((len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']),'private evidence identity')
outcomes=load('token-session-private-outcomes.json');need([r['profile'] for r in outcomes]==PROFILES and all(r['exit-code']==0 for r in outcomes),'private profile outcomes')
launch=load('token-session-private-launch.json');need(launch['state']=='finished' and launch['exit-code']==0,'private launch completion')
post=load('token-session-private-post-run.json');need([r['profile']for r in post]==PROFILES,'private complete profiles')
source=hashlib.sha256((D/'native-token-session-private.cpp').read_bytes()).hexdigest()
for profile,final in zip(PROFILES,post):
 ident=load('token-session-private-'+profile+'-identity.json');exe=final['executable']
 need(final['source']['sha256']==source,'private post-run source')
 need(ident['sha256']==exe['sha256'] and ident['bytes']==exe['bytes']>0,'private post-run PE')
 need(ident['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile] and ident['large-address-aware']==(profile!='x86'),'private PE profile')
 lines=(D/('token-session-private-'+profile+'.log')).read_text().splitlines()
 need(lines[0]==f'identity profile={profile} pointer-bytes={4 if profile.startswith("x86") else 8} private-token-session-controls=1','private identity')
 need(lines[-1]=='complete cases=6 successful-private-session-changes=6 reverted=1','private thread restored')
 rows=[dict(re.findall(r'([\w-]+)=([^\s]*)',line))for line in lines if line.startswith('case ')]
 need([r['requested']for r in rows]==['00000000','00000001','00000007','12345678','80000000','FFFFFFFF'],'private value axes')
 for row in rows:
  need(all(row[k]=='00000000'for k in ['set-status','before-status','after-status','duplicate-status','before']),'successful private calls')
  need(all(row[k]=='4'for k in ['before-length','after-length','duplicate-length']),'private DWORD widths')
  need(row['after']==row['duplicate']==row['requested'] and row['last-error']=='7E11ABCD','exact nonzero scalar and duplicate lifetime')
print('Verified24 private-token value cases,72queries,24sets and restored child threads; guest setter remains unsupported')
