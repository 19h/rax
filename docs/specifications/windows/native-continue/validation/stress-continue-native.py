from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
Q=Path(__file__).parent;R=Path('/Users/int/.assist-continue-20261019');D=R/'docs/specifications/windows/native-continue'
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def change(root,name,old,new):
 p=root/'native'/name;s=p.read_text();v=s.replace(old,new,1);assert v!=s;p.write_text(v)
def edit(root,name,fn):
 p=root/'native'/name;d=load(p);fn(d);p.write_text(json.dumps(d,indent=2)+'\n')
def bind(root):
 p=root/'native/native-continue-v2-launch.json';d=load(p)
 for row in d['steps']:
  raw=(root/'native'/row['log']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 p.write_text(json.dumps(d,indent=2)+'\n');p=root/'native/manifest.json';d=load(p)
 for row in d['files']:
  raw=(root/'native'/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 p.write_text(json.dumps(d,indent=2)+'\n')
def machine(root):
 for suffix in ['before','after']:edit(root,'native-continue-v2-arm64-'+suffix+'.log',lambda d:d.update(machine=0x8664))
def thread(root):
 p=root/'native/native-continue-v2-arm64.log';old=next(x for x in p.read_text().splitlines()if x.startswith('event case=3 '));change(root,p.name,old,re.sub(r'tid=\w+','tid=00000000',old))
def capture(root):
 p=root/'native/native-continue-v2-x86.log';old=next(x for x in p.read_text().splitlines()if x.startswith('capture case=0 '));change(root,p.name,old,re.sub(r'expected-sp=\w+','expected-sp=00000000',old))
controls=[
 ('resume-before-true-apc',lambda r:change(r,'native-continue-v2-arm64.log','resume case=3 test=1 seen=1','resume case=3 test=1 seen=0')),
 ('false-delivers-apc',lambda r:change(r,'native-continue-v2-arm64.log','event case=2 index=0 value=1111 phase=3','event case=2 index=0 value=1111 phase=1')),
 ('existing-alert-priority',lambda r:change(r,'native-continue-v2-arm64.log','resume case=6 test=1 seen=0','resume case=6 test=1 seen=1')),
 ('pending-alert-lost',lambda r:change(r,'native-continue-v2-arm64.log','check case=8 index=0 status=00000101','check case=8 index=0 status=00000000')),
 ('fifo',lambda r:change(r,'native-continue-v2-arm64.log','event case=4 index=0 value=1111','event case=4 index=0 value=2222')),
 ('appended-phase',lambda r:change(r,'native-continue-v2-arm64.log','event case=5 index=2 value=3333 phase=1','event case=5 index=2 value=3333 phase=3')),
 ('last-error-restoration',lambda r:change(r,'native-continue-v2-arm64.log','resume case=9 test=1 seen=1 last=0000ABCD','resume case=9 test=1 seen=1 last=0000C0DE')),
 ('callback-thread',thread),('x86-capture-wrapper',capture),('profile-machine',machine),
 ('boolean-byte',lambda r:change(r,'native-continue-v2-arm64.log','resume case=11 test=255','resume case=11 test=0')),
 ('replay-dataset',lambda r:(r/'cases.json').write_text((r/'cases.json').read_text().replace('00000101','00000000',1))),
 ('source-drift',lambda r:(r/'native/native-continue-v2.cpp').write_bytes((r/'native/native-continue-v2.cpp').read_bytes()+b'\n')),
]
runpy.run_path(str(D/'check_native.py'))['check']();results=[]
with tempfile.TemporaryDirectory(prefix='continue-native-controls-',dir=R/'target')as temporary:
 for name,mutate in controls:
  root=Path(temporary)/name;shutil.copytree(D,root);mutate(root);bind(root)
  try:runpy.run_path(str(root/'check_native.py'))['check']()
  except(AssertionError,KeyError,ValueError)as error:results.append({'name':name,'rejected':True,'error-type':type(error).__name__})
  else:raise AssertionError('Accepted native corruption '+name)
  shutil.rmtree(root);print('Rejected',name,flush=True)
value={'controls':results,'all-rejected':True,'scope':'Native log identities are rebound in launch receipt and native envelope after each mutation; semantic source/ABI/state checks reject the corrupted record.'};(Q/'continue-native-negative-controls.json').write_text(json.dumps(value,indent=2)+'\n')
