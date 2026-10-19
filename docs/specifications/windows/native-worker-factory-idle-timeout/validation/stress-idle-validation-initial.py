from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
Q=Path(__file__).parent;R=Q/'idle-source-epoch2'
D=R/'docs/specifications/windows/native-worker-factory-idle-timeout'
module=runpy.run_path(str(Q/'check-idle-validation.py'));check=module['check']

def change_json(root,name,change):
 p=root/'validation'/name;x=json.loads(p.read_text());change(x);p.write_text(json.dumps(x)+'\n')
def change_log(root,record_name,step_name,change):
 path=root/'validation'/record_name;x=json.loads(path.read_text())
 row=next(r for r in x['steps'] if r.get('name',r.get('gate'))==step_name)
 log=root/'validation'/row['log'];old=log.read_text();new=change(old);assert new!=old
 raw=new.encode();log.write_bytes(raw);row.update(sha256=hashlib.sha256(raw).hexdigest(),bytes=len(raw))
 if 'results' in row:row['results']=re.findall(r'test result: ([^\r\n]+)',new)
 path.write_text(json.dumps(x)+'\n')
def zero_members(root):
 for name in ['linux-idle-epoch2-pre-cpp-archive-identities.json','linux-idle-epoch2-final-owning-archive-identities.json']:
  change_json(root,name,lambda x:x['archives'][0].update(members=0))
def remove_execution(text):
 return re.sub(r'^run (\S*[/\\]rust_archive_link) \1\r?\n','',text,idle=re.M)

def corrupt_native(root):
 p=root/'native/native-worker-factory-idle-v3-arm64.log';old=p.read_text();new=old.replace('status=00000000','status=C000000D',1);assert new!=old;p.write_text(new)
 envelope=root/'native-manifest.json';data=json.loads(envelope.read_text())
 for row in data['files']:
  raw=(root/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 envelope.write_text(json.dumps(data)+'\n')

def corrupt_observer_source(root):
 p=root/'validation/idle-owner-epoch2.rs';p.write_bytes(p.read_bytes()+b'\n// intentional control\n')

controls=[
 ('native-status',corrupt_native),
 ('filtered-full',lambda r:change_log(r,'idle-macos-epoch2-gates.json','full',lambda x:x.replace('0 filtered out;','1 filtered out;',1))),
 ('native-failure-mask',lambda r:change_json(r,'idle-native-epoch2-gates.json',lambda x:x['exact-existing-failures'].pop())),
 ('capi-empty',lambda r:change_log(r,'idle-linux-epoch2-gates.json','capi',lambda x:x.replace('183 passed;','0 passed;',1))),
 ('integration-empty',lambda r:change_log(r,'idle-macos-epoch2-gates.json','integration',lambda x:x.replace('544 passed;','0 passed;',1))),
 ('all-targets-removed',lambda r:change_json(r,'idle-macos-epoch2-gates.json',lambda x:next(row for row in x['steps'] if row['name']=='all-targets')['argv'].remove('--all-targets'))),
 ('baseline-success',lambda r:change_json(r,'idle-baseline-proof.json',lambda x:x.update(state='finished',**{'exit-code':0}))),
 ('ctest-progress',lambda r:change_log(r,'macos-idle-epoch2-owning-gates.json','ctest',lambda x:x.replace('1/7 Test','0/7 Test',1))),
 ('consumer-not-executed',lambda r:change_log(r,'linux-idle-epoch2-owning-gates.json','cpp',remove_execution)),
 ('core-feature-drift',lambda r:change_json(r,'native-idle-epoch2-final-owning-archive-identities.json',lambda x:x.update(**{'core-features':['smir-jit']}))),
 ('empty-archives',zero_members),
 ('guest-exit-confusion',lambda r:change_json(r,'ordinary-program-summary.json',lambda x:x.update(**{'ordinary-guest-exit-success':not x['ordinary-guest-exit-success']}))),
 ('ordinary-turn-drift',lambda r:change_json(r,'ordinary-program-summary.json',lambda x:x['programs'][0].update(**{'additional-observer-turns':360}))),
 ('observer-source-drift',corrupt_observer_source),
 ('native-leaf-exception-drift',lambda r:change_json(r,'../idle-epoch2-correction.json',lambda x:x.update(**{'after-sha256':'0'*64}))),
 ('engineering-append-drift',lambda r:change_json(r,'engineering-source-updates.json',lambda x:x.update(**{'after-sha256':'0'*64}))),
 ('source-hash-drift',lambda r:change_json(r,'idle-candidate-epoch2.json',lambda x:x['files'][0].update(sha256='0'*64))),
]
check(R,D)
rows=[]
with tempfile.TemporaryDirectory(prefix='user-validation-controls-',dir=Q) as temp:
 for name,mutate in controls:
  root=Path(temp)/name;shutil.copytree(D,root);mutate(root)
  envelope = root/'evidence-manifest.json'
  if envelope.exists():
   data=json.loads(envelope.read_text())
   for item in data['files']:
    raw=(root/item['path']).read_bytes();item.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
   envelope.write_text(json.dumps(data)+'\n')
  try:check(R,root)
  except (AssertionError,ValueError,KeyError) as exc:rows.append({'name':name,'rejected':True,'error-type':type(exc).__name__})
  else:raise AssertionError('validation corruption accepted: '+name)
(Q/'idle-final-negative-controls.json').write_text(json.dumps({'controls':rows,'all-rejected':True,'scope':'Logical controls rebind mutated log hashes and result metadata, so rejection cannot rely only on the outer log digest.'},indent=2)+'\n')
print('rejected',len(rows),'validation corruptions')
