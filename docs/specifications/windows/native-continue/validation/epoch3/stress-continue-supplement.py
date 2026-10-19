from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
Q=Path(__file__).parent;R=Path('/Users/int/.assist-continue-20261019');D=R/'docs/specifications/windows/native-continue'
def change(root,old,new):
 p=root/'native-supplement/native-continue-v3-arm64.log';s=p.read_text();v=s.replace(old,new,1);assert v!=s;p.write_text(v)
def target(root):
 p=root/'native-supplement/native-continue-v3-arm64.log';line=next(x for x in p.read_text().splitlines()if x.startswith('abandon case=13 '));change(root,line,re.sub(r'target-pc=\w+','target-pc=0000000000000000',line))
def thread(root):
 p=root/'native-supplement/native-continue-v3-arm64.log';line=next(x for x in p.read_text().splitlines()if x.startswith('event case=13 index=0 '));change(root,line,re.sub(r'tid=\w+','tid=00000000',line))
def bind(root):
 N=root/'native-supplement';p=N/'native-continue-v3-launch.json';d=json.loads(p.read_text())
 for row in d['steps']:
  raw=(N/row['log']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 p.write_text(json.dumps(d,indent=2)+'\n');p=N/'manifest.json';d=json.loads(p.read_text())
 for row in d['files']:
  raw=(N/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 p.write_text(json.dumps(d,indent=2)+'\n')
controls=[('false-abandonment-delivers-early',lambda r:change(r,'event case=13 index=1 value=2222 phase=3','event case=13 index=1 value=2222 phase=1')),('true-abandonment-delivers-late',lambda r:change(r,'event case=14 index=1 value=2222 phase=1','event case=14 index=1 value=2222 phase=3')),('inside-alert-priority',lambda r:change(r,'resume case=16 test=1 seen=1','resume case=16 test=1 seen=2')),('inside-alert-status',lambda r:change(r,'inside-alert case=16 status=00000000','inside-alert case=16 status=C0000022')),('null-saved-target',target),('native-worker-exit',lambda r:change(r,'worker case=15 wait=00000000 exit=0','worker case=15 wait=00000000 exit=1')),('callback-thread',thread),('supplement-projection',lambda r:(r/'supplement-cases.json').write_text((r/'supplement-cases.json').read_text().replace('2222','3333',1))),('supplement-source',lambda r:(r/'native-supplement/native-continue-v3.cpp').write_bytes((r/'native-supplement/native-continue-v3.cpp').read_bytes()+b'\n'))]
runpy.run_path(str(D/'check_supplement.py'))['check']();rows=[]
with tempfile.TemporaryDirectory(prefix='continue-supplement-controls-',dir=R/'target')as temporary:
 for name,mutate in controls:
  root=Path(temporary)/name;shutil.copytree(D,root);mutate(root);bind(root)
  try:runpy.run_path(str(root/'check_supplement.py'))['check']()
  except(AssertionError,KeyError,ValueError)as error:rows.append({'name':name,'rejected':True,'error-type':type(error).__name__})
  else:raise AssertionError('Accepted corruption '+name)
  shutil.rmtree(root);print('Rejected',name,flush=True)
(Q/'continue-supplement-negative-controls.json').write_text(json.dumps({'controls':rows,'all-rejected':True,'scope':'Supplement launch/log/outer hashes rebound; independent native order/context/source/profile checks remain unchanged.'},indent=2)+'\n')
