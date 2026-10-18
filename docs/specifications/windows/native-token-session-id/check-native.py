"""Bind four native SessionId producers without implying guest execution."""
from pathlib import Path
import hashlib,json,re
D=Path(__file__).resolve().parent
FIELDS=['kind','role','length','status','output-bytes-cells','return-bytes-cells','output-protect','returned-protect']
def load(name):
 raw=(D/name).read_bytes()
 return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw.decode('utf-8-sig'))
def need(value,why):
 if not value:raise ValueError(why)
need(__debug__,'optimized checker rejected')
seen=set()
for row in load('native-inputs.json')['files'] + load('primary-inputs.json')['files']:
 path=row['path'];need(path not in seen and not Path(path).is_absolute() and '..' not in Path(path).parts,'relative unique evidence path');seen.add(path)
 raw=(D/path).read_bytes();need((len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']),'evidence identity '+path)
profiles=['arm64','x64','x86','x86-laa'];corpus=load('cases.json');native=load('native-identities.json');post=load('token-session-id-post-run.json')
need(corpus['class']==12 and corpus['profiles']==profiles and corpus['per-profile']==1830 and len(corpus['cases'])==1830,'complete class12 corpus')
outcomes=load('token-session-id-outcomes.json');need([r['profile'] for r in outcomes]==profiles and all(r['exit-code']==0 for r in outcomes),'native profile outcomes')
launch=load('token-session-id-launch.json');need(launch['state']=='finished' and launch['exit-code']==0,'native launch completion')
need(native['calls']==7320 and [p['profile']for p in native['producers']]==profiles,'four native producers')
need([r['profile'] for r in post] == profiles, 'complete post-run native profiles')
source=hashlib.sha256((D/'native-token-session-id.cpp').read_bytes()).hexdigest()
for profile,producer,final in zip(profiles,native['producers'],post):
 need(final['profile']==profile and producer['source']['sha256']==final['source']['sha256']==source,'post-run native source')
 ident=load('token-session-id-'+profile+'-identity.json');exe=final['executable']
 need(ident['sha256']==exe['sha256']==producer['executable']['sha256'] and ident['bytes']==exe['bytes']>0,'post-run native executable')
 need(ident['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile] and ident['large-address-aware']==(profile!='x86'),'native machine and LAA')
 text=(D/('token-session-id-'+profile+'.log')).read_text();lines=text.splitlines()
 need(lines[0]=='reference class=12 session=00000000 bytes=4','measured native session value')
 need(lines[1]==f'identity profile={profile} pointer-bytes={4 if profile.startswith("x86") else 8} build=10.0.29683 page=4096 cases=1830','native host and profile')
 need(lines[-1]=='complete cases=1830 closed-handles=3 released-arenas=1','complete native cleanup')
 rows=[dict(re.findall(r'([\w-]+)=([^\s]*)',x))for x in lines if x.startswith('case ')]
 need([int(r['ordinal'])for r in rows]==list(range(1830)),'complete ordered rows')
 need(all(r['escaped']=='00000000' and r['last-error']=='7E11ABCD'for r in rows),'no escaped exception or last-error change')
 need([{k:r[k]for k in FIELDS}for r in rows]==corpus['cases'],'raw native semantic rows')
 need(producer['calls']==1830 and producer['closed-handles']==3 and producer['released-arenas']==1 and producer['captured-session-id']==0,'producer totals')
kinds=['primary','thread-token','effective','null','process','thread','invalid','event','real','no-access'];lengths=[0,1,2,3,4,5,8,15,16,4096,0xffffffff]
axes=[(k,'0',f'{n:08X}')for k in kinds for n in lengths]+[(k,str(r),f'{n:08X}')for k in kinds for r in range(43)for n in [0,4,8,0xffffffff]]
need([(r['kind'],r['role'],r['length'])for r in corpus['cases']]==axes,'complete handle/length/pointer axes')
for kind in ['primary','effective','real']:
 row=next(r for r in corpus['cases']if (r['kind'],r['role'],r['length'])==(kind,'0','00000004'))
 need(row['status']=='00000000' and row['output-bytes-cells']=='00000000' and row['return-bytes-cells']=='04000000','successful SessionId DWORD and required length')
import subprocess,sys
subprocess.run([sys.executable, '-B', str(D/'check-private.py')], check=True)
print('Verified7320 native SessionId queries and four source/PE/log identities; guest validation is separate')
