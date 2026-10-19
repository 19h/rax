from pathlib import Path
import json,runpy,shutil,tempfile
Q=Path(__file__).parent
D=Q/'registry-source/docs/specifications/windows/native-state-separation-registry'
module=runpy.run_path(str(Q/'check-native-redirection.py'))
check,load=module['check'],module['load']

def text_change(root,name,before,after):
 p=root/name;s=p.read_text();assert before in s;p.write_text(s.replace(before,after,1))
def json_change(root,name,change):
 p=root/name;x=load(p);change(x);p.write_text(json.dumps(x)+'\n')

stem='native-redirection-keys-v1'
controls=[
 ('wrong-status',lambda r:text_change(r,stem+'-arm64.log','status=C0000034','status=00000000')),
 ('output-nonzero',lambda r:text_change(r,stem+'-arm64.log','out=0000000000000000','out=0000000000000001')),
 ('duplicate-case',lambda r:text_change(r,stem+'-arm64.log','sample=0 path=1 desired=00000001','sample=0 path=0 desired=00000001')),
 ('wrong-pointer-width',lambda r:text_change(r,stem+'-x86.log','POINTER_BYTES 4','POINTER_BYTES 8')),
 ('handle-leak',lambda r:text_change(r,stem+'-arm64.log','handles-after=49','handles-after=50')),
 ('source-mutation',lambda r:text_change(r,stem+'.cpp','GetProcessHandleCount','GetCurrentProcessId')),
 ('after-source-hash',lambda r:json_change(r,stem+'-arm64-source-after.json',lambda x:x.update(Hash='0'*64))),
 ('pe-machine',lambda r:json_change(r,stem+'-arm64-before.json',lambda x:x.update(machine=0x8664))),
 ('pe-after-hash',lambda r:json_change(r,stem+'-x64-after.json',lambda x:x.update(sha256='0'*64))),
 ('module-after-hash',lambda r:json_change(r,stem+'-modules-after.json',lambda x:x[0].update(Hash='0'*64))),
 ('producer-exit',lambda r:json_change(r,stem+'-outcomes.json',lambda x:x[0].update(**{'exit-code':1}))),
 ('partial-launch',lambda r:json_change(r,stem+'-launch.json',lambda x:x.update(state='running'))),
]
assert check(D)['cases']==256
rows=[]
with tempfile.TemporaryDirectory(prefix='registry-native-controls-',dir=Q) as temp:
 for name,mutate in controls:
  root=Path(temp)/name;shutil.copytree(D,root);mutate(root)
  try:check(root)
  except (AssertionError,ValueError,KeyError) as exc:rows.append({'name':name,'rejected':True,'error-type':type(exc).__name__})
  else:raise AssertionError('native corruption accepted: '+name)
(Q/'registry-native-negative-controls.json').write_text(json.dumps({'controls':rows,'all-rejected':True},indent=2)+'\n')
print('rejected',len(rows),'native evidence corruptions')
