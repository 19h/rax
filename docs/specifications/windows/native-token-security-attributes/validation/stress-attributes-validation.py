from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
K=Path(__file__).parent;R=Path('/Users/int/.assist-token-20261019');D=R/'docs/specifications/windows/native-token-security-attributes'
check=runpy.run_path(str(K/'check-attributes-validation.py'))['check']
def change(root,name,mutate):
 p=root/'validation'/name;d=json.loads(p.read_text());mutate(d);p.write_bytes((json.dumps(d)+'\n').encode())
def log(root,name,step,mutate):
 p=root/'validation'/name;d=json.loads(p.read_text());r=next(x for x in d['steps'] if x.get('name',x.get('gate'))==step);q=root/'validation'/r['log'];old=q.read_text();new=mutate(old);assert new!=old;raw=new.encode();q.write_bytes(raw);r.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest());r['results']=re.findall(r'test result: ([^\r\n]+)',new);p.write_bytes((json.dumps(d)+'\n').encode())
def empty_archives(root):
 for name in ['linux-attributes-epoch2-pre-cpp-archive-identities.json','linux-attributes-epoch2-final-owning-archive-identities.json']:
  change(root,name,lambda d:d['archives'][0].update(members=0))
def observer(root):
 p=root/'validation/attributes-owner-epoch2.rs';p.write_bytes(p.read_bytes()+b'\n// intentional mutation\n')
def source_manifest(root):change(root,'attributes-candidate-epoch2.json',lambda d:d['files'][0].update(sha256='0'*64))
controls=[
 ('filtered-full',lambda r:log(r,'attributes-macos-epoch2-gates.json','full',lambda t:t.replace('0 filtered out;','1 filtered out;',1))),
 ('native-failure-mask',lambda r:change(r,'attributes-native-epoch2-gates.json',lambda d:d['exact-existing-failures'].pop())),
 ('capi-empty',lambda r:log(r,'attributes-linux-epoch2-gates.json','capi',lambda t:t.replace('183 passed;','0 passed;',1))),
 ('integration-empty',lambda r:log(r,'attributes-macos-epoch2-gates.json','integration',lambda t:t.replace('544 passed;','0 passed;',1))),
 ('all-targets-removed',lambda r:change(r,'attributes-macos-epoch2-gates.json',lambda d:next(x for x in d['steps'] if x['name']=='all-targets')['argv'].remove('--all-targets'))),
 ('baseline-success',lambda r:change(r,'attributes-baseline-proof.json',lambda d:d.update(state='finished',passed=3,failed=0))),
 ('ctest-progress',lambda r:log(r,'macos-attributes-epoch2-owning-gates-c.json','ctest',lambda t:t.replace('1/7 Test','0/7 Test',1))),
 ('consumer-not-executed',lambda r:log(r,'linux-attributes-epoch2-owning-gates-b.json','cpp',lambda t:re.sub(r'^run (\S*[/\\]rust_archive_link) \1\r?\n','',t,flags=re.M))),
 ('core-feature-drift',lambda r:change(r,'native-attributes-epoch2-d-final-owning-archive-identities.json',lambda d:d.update(**{'core-features':['smir-jit']}))),
 ('empty-archives',empty_archives),
 ('guest-exit-confusion',lambda r:change(r,'ordinary-program-summary.json',lambda d:d.update(**{'ordinary-guest-exit-success':True}))),
 ('ordinary-turn-drift',lambda r:change(r,'ordinary-program-summary.json',lambda d:d['programs'][0].update(**{'additional-observer-turns':44005}))),
 ('observer-source-drift',observer),
 ('engineering-record-drift',lambda r:change(r,'engineering-source-updates.json',lambda d:d['files'][0].update(**{'after-sha256':'0'*64}))),
 ('source-hash-drift',source_manifest),
]
check(R,D);rows=[]
with tempfile.TemporaryDirectory(prefix='attributes-validation-controls-',dir=R/'build-tmp') as temporary:
 for name,mutate in controls:
  root=Path(temporary)/name;shutil.copytree(D,root);mutate(root)
  envelope=root/'evidence-manifest.json'
  if envelope.exists():
   d=json.loads(envelope.read_text())
   for x in d['files']:
    raw=(root/x['path']).read_bytes();x.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
   envelope.write_bytes((json.dumps(d)+'\n').encode())
  try:check(R,root)
  except (AssertionError,ValueError,KeyError) as exc:rows.append({'name':name,'rejected':True,'error-type':type(exc).__name__})
  else:raise AssertionError('Accepted corruption '+name)
  shutil.rmtree(root)
  print('Rejected',name,flush=True)
(K/'attributes-final-negative-controls.json').write_text(json.dumps({'controls':rows,'all-rejected':True,'scope':'Logical corruptions rebind changed log digests/results and outer evidence manifest; rejection also requires semantic checks.'},indent=2)+'\n')
