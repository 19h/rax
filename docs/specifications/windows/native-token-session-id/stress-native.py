"""Reject altered native claims after rebinding the evidence digests."""
from pathlib import Path
import json,hashlib,shutil,subprocess,sys,tempfile
D=Path(__file__).resolve().parent

def load(p):
 raw=p.read_bytes()
 return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw.decode('utf-8-sig'))
def save(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def json_change(root,name,fn):
 p=root/name;v=load(p);fn(v);save(p,v)
def text_change(root,name,a,b):
 p=root/name;s=p.read_text();assert a in s;p.write_text(s.replace(a,b,1))
def wrong_success(root):
 for p in ['arm64','x64','x86','x86-laa']:
  path=root/('token-session-id-'+p+'.log');lines=path.read_text().splitlines()
  for i,line in enumerate(lines):
   if line.startswith('case ordinal=4 '):lines[i]=line.replace('status=00000000','status=C0000022',1)
  path.write_text('\n'.join(lines)+'\n')
 json_change(root,'cases.json',lambda v:v['cases'][4].update(status='C0000022'))
def bind(root):
 for name in ['native-inputs.json','private-inputs.json','primary-inputs.json']:
  p=root/name;v=load(p)
  for row in v['files']:
   raw=(root/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
  save(p,v)
cases=[
 ('missing-profile',lambda r:json_change(r,'native-identities.json',lambda v:v['producers'].pop())),
 ('missing-post-run-profile',lambda r:json_change(r,'token-session-id-post-run.json',lambda v:v.pop())),
 ('wrong-machine',lambda r:json_change(r,'token-session-id-arm64-identity.json',lambda v:v.update(machine=0x14c))),
 ('wrong-success-status',wrong_success),
 ('missing-row',lambda r:text_change(r,'token-session-id-arm64.log','case ordinal=0 ','discarded ordinal=0 ')),
 ('wrong-last-error',lambda r:text_change(r,'token-session-id-arm64.log','last-error=7E11ABCD','last-error=00000000')),
 ('wrong-native-value',lambda r:text_change(r,'token-session-id-arm64.log','session=00000000','session=00000001')),
 ('changed-source',lambda r:text_change(r,'native-token-session-id.cpp','#define NOMINMAX','#define NOMINMAX\n// changed')),
 ('wrong-private-value',lambda r:text_change(r,'token-session-private-arm64.log','after=00000007','after=00000008')),
 ('private-thread-not-reverted',lambda r:text_change(r,'token-session-private-arm64.log','reverted=1','reverted=0')),
 ('private-missing-high-bit',lambda r:text_change(r,'token-session-private-arm64.log','case requested=80000000','discarded requested=80000000')),
]
results=[]
for name,mutate in cases:
 with tempfile.TemporaryDirectory(prefix='token-session-proof-')as temp:
  root=Path(temp)/'proof';shutil.copytree(D,root,ignore=shutil.ignore_patterns('__pycache__','validation'))
  mutate(root);bind(root)
  result=subprocess.run([sys.executable,'-B',str(root/'check-native.py')],capture_output=True,text=True)
  if result.returncode==0:raise RuntimeError('accepted altered proof '+name)
  results.append({'case':name,'rejected':True,'error':result.stderr.splitlines()[-1]})
print(json.dumps(results,indent=2))
