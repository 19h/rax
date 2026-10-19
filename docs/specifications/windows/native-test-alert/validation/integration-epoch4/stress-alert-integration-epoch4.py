from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
P=Path(__file__).parent;R=Path('/Users/int/.assist-alert3-20261019');D=R/'docs/specifications/windows/native-test-alert'
def value(root,name):return root/'validation/integration-epoch4'/name
def edit(root,name,fn):
 p=value(root,name);d=json.loads(p.read_text());fn(d);p.write_text(json.dumps(d,indent=2)+'\n')
def logchange(root,record,name,old,new):
 p=value(root,record);d=json.loads(p.read_text());row=next(x for x in d['steps']if x.get('name',x.get('gate'))==name);f=value(root,row['log']);s=f.read_text();t=s.replace(old,new);assert t!=s;f.write_text(t);raw=f.read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest());row['results']=re.findall(r'test result: ([^\r\n]+)',t)
 for outcome in d.get('outcomes',[]):
  if outcome['log']==f.name:outcome['log-sha256']=row['sha256']
 p.write_text(json.dumps(d,indent=2)+'\n')
def counts(root):
 logchange(root,'gates-macos.json','full','8070 passed;','8069 passed;');edit(root,'summary.json',lambda d:d['library-gates']['macos'].update(passed=8069))
def windows(root):
 old='smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime';new=old+'_unrecognized';logchange(root,'gates-windows.json','full',old,new);edit(root,'gates-windows.json',lambda d:d.update({'exact-existing-failures':[new if x==old else x for x in d['exact-existing-failures']]}))
def zero_walk(root):
 for part in ['before','after']:edit(root,'archives-windows-'+part+'.json',lambda d:d['archives'][0].update(members=0))
controls=[('coherent-current-library-count',counts),('filtered-current-full',lambda r:logchange(r,'gates-linux.json','full','0 filtered out;','1 filtered out;')),('unexpected-current-windows-failure',windows),('current-capi-count',lambda r:logchange(r,'gates-macos.json','capi','183 passed;','182 passed;')),('current-owning-core-drift',lambda r:edit(r,'ordinary.json',lambda d:d['core-after'].update(sha256='0'*64))),('zero-current-archive-walk',zero_walk),('current-source-hash',lambda r:edit(r,'source-freeze.json',lambda d:d['files'][0].update(sha256='0'*64))),('false-full-process-completion',lambda r:edit(r,'summary.json',lambda d:d.update({'full-process-goal-complete':True}))),('false-whole-current-root-validation',lambda r:edit(r,'summary.json',lambda d:d.update({'whole-current-root-products-validated':True}))),('false-unchanged-feature',lambda r:edit(r,'summary.json',lambda d:d.update({'feature-code-unchanged':False})))]
runpy.run_path(str(D/'check_integration.py'))['check']('macos',R);results=[]
with tempfile.TemporaryDirectory(prefix='alert-integration-controls-',dir=R/'target')as temporary:
 for name,mutate in controls:
  root=Path(temporary)/name;shutil.copytree(D,root);mutate(root)
  try:runpy.run_path(str(root/'check_integration.py'))['check']('macos',R)
  except(AssertionError,ValueError,KeyError)as error:results.append({'name':name,'rejected':True,'error-type':type(error).__name__})
  else:raise AssertionError('Accepted integration corruption '+name)
  shutil.rmtree(root);print('Rejected',name,flush=True)
(P/'alert-integration-negative-controls-epoch4.json').write_text(json.dumps({'controls':results,'all-rejected':True,'scope':'Fresh integration checker is invoked independently of the outer envelope. Relevant launch/log hashes and coherent summary counts are rebound; source/checker semantics remain fixed.'},indent=2)+'\n')
