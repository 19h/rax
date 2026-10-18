"""Reject altered semantic evidence after rebinding file and group digests."""
from pathlib import Path
import hashlib,json,shutil,subprocess,sys,tempfile
D=Path(__file__).resolve().parent

def load(p):
 raw=p.read_bytes();return json.loads(raw.decode('utf-16') if raw[:2]in[b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
def save(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def edit(root,name,fn):
 p=root/name;v=load(p);fn(v);save(p,v)
def text(root,name,a,b):
 p=root/name;s=p.read_text();assert a in s;p.write_text(s.replace(a,b,1))
def bind(root):
 p=root/'cases.json';v=load(p)
 for g in v['groups']:g['sha256']=hashlib.sha256(json.dumps(g['cases'],sort_keys=True,separators=(',',':')).encode()).hexdigest()
 save(p,v)
 for name in ['native-inputs.json','primary-inputs.json']:
  p=root/name;v=load(p)
  for r in v['files']:
   raw=(root/r['path']).read_bytes();r.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
  save(p,v)
def byte_width(root):
 for p in ['arm64','x64','x86','x86-laa']:
  text(root,f'native/token-scalar-survey-v2-{p}-class21.log','value=A5A5A501 bytes=1','value=A5A5A501 bytes=4')
 edit(root,'cases.json',lambda v:[v['references'][p]['21'].update(returned=4)for p in v['profiles']])
def elevation(root):
 for p in ['arm64','x64','x86','x86-laa']:
  path=root/f'native/token-scalar-survey-v2-{p}-class20.log';lines=path.read_text().splitlines()
  for i,l in enumerate(lines):
   if l.startswith('case ordinal=0 '):lines[i]=l.replace('return-bytes-cells=04000000','return-bytes-cells=A5A5A5A5')
  path.write_text('\n'.join(lines)+'\n')
 edit(root,'cases.json',lambda v:v['groups'][v['mapping']['arm64']['20']]['cases'][0].update({'return-bytes-cells':'A5A5A5A5'}))
def wow_probe(root):
 for p in ['x86','x86-laa']:
  for c in [40,45]:text(root,f'native/token-scalar-survey-v2-{p}-class{c}.log','status=C0000003','status=C0000005')
 edit(root,'cases.json',lambda v:v['groups'][v['mapping']['x86']['40']]['cases'][0].update(status='C0000005'))
cases=[
 ('missing-post-profile',lambda r:edit(r,'native/token-scalar-survey-v2-post-run.json',lambda v:v.pop())),
 ('wrong-PE-machine',lambda r:edit(r,'native/token-scalar-survey-v2-arm64-identity.json',lambda v:v.update(machine=0x14c))),
 ('invented-DWORD-width',byte_width),
 ('wrong-elevation-length-priority',elevation),
 ('wrong-WoW64-probe-priority',wow_probe),
 ('invented-private-DWORD-preservation',lambda r:text(r,'private/token-namespace-private-arm64.log','after=00000001 after-length=4 duplicate-status=00000000 duplicate=00000001','after=00000007 after-length=4 duplicate-status=00000000 duplicate=00000007')),
]
results=[]
for name,mutate in cases:
 with tempfile.TemporaryDirectory(prefix='native-scalar-proof-')as temp:
  root=Path(temp)/'proof';shutil.copytree(D,root,ignore=shutil.ignore_patterns('__pycache__','validation'))
  mutate(root);bind(root)
  p=subprocess.run([sys.executable,'-B',str(root/'check-native.py')],capture_output=True,text=True)
  if p.returncode==0:raise RuntimeError('accepted altered evidence '+name)
  results.append({'case':name,'rejected':True,'error':p.stderr.splitlines()[-1]})
print(json.dumps(results,indent=2))
