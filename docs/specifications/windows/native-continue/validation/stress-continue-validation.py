from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
Q=Path(__file__).parent;R=Path('/Users/int/.assist-continue-20261019');D=R/'docs/specifications/windows/native-continue'
def edit(root,name,fn):
 p=root/'validation'/name;d=json.loads(p.read_text());fn(d);p.write_text(json.dumps(d,indent=2)+'\n')
def logchange(root,record,name,old,new):
 p=root/'validation'/record;d=json.loads(p.read_text());row=next(x for x in d['steps']if x.get('name',x.get('gate'))==name);f=root/'validation'/row['log'];s=f.read_text();t=s.replace(old,new);assert t!=s;f.write_text(t);raw=f.read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest());row['results']=re.findall(r'test result: ([^\r\n]+)',t)
 for x in d.get('outcomes',[]):
  if x['log']==f.name:x['log-sha256']=row['sha256']
 p.write_text(json.dumps(d,indent=2)+'\n')
def bind(root):
 p=root/'evidence-manifest.json';d=json.loads(p.read_text())
 for row in d['files']:
  raw=(root/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 p.write_text(json.dumps(d,indent=2)+'\n')
def counts(root):
 logchange(root,'gates-macos.json','full','8079 passed;','8078 passed;');edit(root,'summary.json',lambda d:d['library-gates']['macos'].update(passed=8078))
def windows(root):
 old='smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime';new=old+'_unknown';logchange(root,'gates-windows.json','full',old,new);edit(root,'gates-windows.json',lambda d:d.update({'exact-existing-failures':[new if x==old else x for x in d['exact-existing-failures']]}))
def zero_walk(root):
 for suffix in['before','after']:edit(root,'archives-windows-'+suffix+'.json',lambda d:d['archives'][0].update(members=0))
def native_failure(root):
 def mutate(d):
  for suffix in['before','after']:d['outcomes'][0]['native-'+suffix]['exit-code']=1
 edit(root,'ordinary.json',mutate)
def wrong_gpr(root):
 p=root/'validation/ordinary.json';d=json.loads(p.read_text());step=next(x for x in d['steps']if x['name']=='smoke');s=(root/'validation'/step['log']).read_text();line=next(x for x in s.splitlines()if x.startswith('continue-result '));new=re.sub(r'gpr0=0x[0-9a-f]+','gpr0=0x0',line);logchange(root,'ordinary.json','smoke',line,new)
def wrong_sp(root):
 p=root/'validation/ordinary.json';d=json.loads(p.read_text());step=next(x for x in d['steps']if x['name']=='smoke');s=(root/'validation'/step['log']).read_text();line=next(x for x in s.splitlines()if x.startswith('continue-result '));new=re.sub(r'sp=0x[0-9a-f]+','sp=0x93fbd0',line);logchange(root,'ordinary.json','smoke',line,new)
controls=[('coherent-library-count',counts),('filtered-full-selection',lambda r:logchange(r,'gates-linux.json','full','0 filtered out;','1 filtered out;')),('unrecognized-windows-failure',windows),('capi-count',lambda r:logchange(r,'gates-macos.json','capi','183 passed;','182 passed;')),('integration-count',lambda r:logchange(r,'gates-linux.json','integration','544 passed;','543 passed;')),('frozen-source-hash',lambda r:edit(r,'source-freeze.json',lambda d:d['files'][0].update(sha256='0'*64))),('parent-compiled-intersection',lambda r:edit(r,'continue-parent-publication-integration.json',lambda d:d.update({'compiled-input-intersection':['src/user/windows/dll/native.rs']}))),('owning-helper-hash',lambda r:edit(r,'owning-source-hashes.json',lambda d:d['helpers'][0].update(sha256='0'*64))),('zero-archive-walk',zero_walk),('owning-core-drift',lambda r:edit(r,'ordinary.json',lambda d:d['core-after'].update(sha256='0'*64))),('native-control-failure',native_failure),('wrong-smoke-exit',lambda r:logchange(r,'ordinary.json','smoke','Complete(Exited(0))','Complete(Exited(1))')),('fabricated-zero-result-register',wrong_gpr),('wrong-restored-stack',wrong_sp),('false-whole-root-validation',lambda r:edit(r,'summary.json',lambda d:d.update({'whole-current-root-products-validated':True}))),('false-full-process-completion',lambda r:edit(r,'summary.json',lambda d:d.update({'full-process-goal-complete':True}))),('retained-byte-copy-drift',lambda r:edit(r,'continue-native-retained-artifacts.json',lambda d:d['artifacts'][0]['retained'].update(sha256='0'*64))),('kernel-profile-drift',lambda r:edit(r,'continue-kernel-profile.json',lambda d:d.update(build=1)))]
runpy.run_path(str(D/'check_validation.py'))['check']('macos',R);results=[]
with tempfile.TemporaryDirectory(prefix='continue-validation-controls-',dir=R/'target')as temporary:
 for name,mutate in controls:
  root=Path(temporary)/name;shutil.copytree(D,root);mutate(root);bind(root)
  try:runpy.run_path(str(root/'check_validation.py'))['check']('macos',R)
  except(AssertionError,ValueError,KeyError)as error:results.append({'name':name,'rejected':True,'error-type':type(error).__name__})
  else:raise AssertionError('Accepted corrupted evidence '+name)
  shutil.rmtree(root);print('Rejected',name,flush=True)
(Q/'continue-validation-negative-controls.json').write_text(json.dumps({'controls':results,'all-rejected':True,'scope':'Outer envelope and relevant log/receipt hashes are rebound after each corruption; coherent counts are changed together. Source/checker semantics remain fixed.'},indent=2)+'\n')
