from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
Q=Path(__file__).parent;R=Path('/Users/int/.assist-continue-20261019');D=R/'docs/specifications/windows/native-continue'
def edit(root,name,fn):
 p=root/'validation/epoch3'/name;d=json.loads(p.read_text());fn(d);p.write_text(json.dumps(d,indent=2)+'\n')
def logchange(root,record,name,old,new):
 v=root/'validation/epoch3';p=v/record;d=json.loads(p.read_text());row=next(x for x in d['steps']if x.get('name',x.get('gate'))==name);f=v/row['log'];s=f.read_text();t=s.replace(old,new);assert t!=s;f.write_text(t);raw=f.read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 if 'results'in row:row['results']=re.findall(r'test result: ([^\r\n]+)',t)
 for x in d.get('outcomes',[]):
  if x['log']==f.name:x['log-sha256']=row['sha256']
 p.write_text(json.dumps(d,indent=2)+'\n')
def counts(root):
 logchange(root,'gates-macos.json','full','8082 passed;','8081 passed;');edit(root,'summary.json',lambda d:d['library-gates']['macos'].update(passed=8081))
def windows(root):
 old='smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime';new=old+'_unknown';logchange(root,'gates-windows.json','full',old,new);edit(root,'gates-windows.json',lambda d:d.update({'exact-existing-failures':[new if x==old else x for x in d['exact-existing-failures']]}))
def zero(root):
 for suffix in['before','after']:edit(root,'archives-windows-'+suffix+'.json',lambda d:d['archives'][0].update(members=0))
def restored(root):
 v=root/'validation/epoch3';d=json.loads((v/'ordinary.json').read_text());row=next(x for x in d['steps']if x['name']=='smoke');out=(v/row['log']).read_text();old=next(x for x in out.splitlines()if x.startswith('continue-result '));new=re.sub(r'sp=0x[0-9a-f]+','sp=0x93fbd0',old);logchange(root,'ordinary.json','smoke',old,new)
def capi(root):
 v=root/'validation/epoch3';d=json.loads((v/'capi.json').read_text());out=(v/d['steps'][1]['log']).read_text();old=next(x for x in out.splitlines()if x.startswith('api-result '));row=json.loads(old[len('api-result '):]);row['reason']=5;new='api-result '+json.dumps(row,separators=(',',':'));logchange(root,'capi.json','ordinary',old,new);edit(root,'capi.json',lambda d:d['outcomes'][0].update(reason=5))
controls=[('coherent-library-count',counts),('filtered-full-selection',lambda r:logchange(r,'gates-linux.json','full','0 filtered out;','1 filtered out;')),('unrecognized-windows-failure',windows),('rejected-wait-regression-lost',lambda r:logchange(r,'rejected-epoch2-macos.json','full','apc_restoration_preserves_same_height_wait_parent_all_abis','different_test')),('unconfirmed-ownership-assumption',lambda r:edit(r,'assumptions.json',lambda d:d['assumptions'][0].update(status='retained'))),('upstream-feature-overlap',lambda r:edit(r,'continue-upstream-integration-epoch3.json',lambda d:d['files'][0].update(path=d['protected-feature-paths'][0]))),('zero-archive-walk',zero),('owning-core-drift',lambda r:edit(r,'ordinary.json',lambda d:d['core-after'].update(sha256='0'*64))),('wrong-smoke-exit',lambda r:logchange(r,'ordinary.json','smoke','Complete(Exited(0))','Complete(Exited(1))')),('wrong-restored-stack',restored),('capi-success-converted-to-failure',capi),('false-full-process-completion',lambda r:edit(r,'summary.json',lambda d:d.update({'full-process-goal-complete':True})))]
runpy.run_path(str(D/'check_epoch3.py'))['check']('macos',R);results=[]
with tempfile.TemporaryDirectory(prefix='continue-epoch3-controls-',dir=R/'target')as temporary:
 for name,mutate in controls:
  root=Path(temporary)/name;shutil.copytree(D,root);mutate(root)
  try:runpy.run_path(str(root/'check_epoch3.py'))['check']('macos',R)
  except(AssertionError,ValueError,KeyError)as error:results.append({'name':name,'rejected':True,'error-type':type(error).__name__})
  else:raise AssertionError('Accepted corrupted current evidence '+name)
  shutil.rmtree(root);print('Rejected',name,flush=True)
value={'controls':results,'all-rejected':True,'scope':'Direct current checker evaluated with relevant log/receipt hashes and coherent result rows rebound; no outer envelope rejection substitutes for current semantics.'};(Q/'continue-current-negative-controls.json').write_text(json.dumps(value,indent=2)+'\n');shutil.copy2(Q/'continue-current-negative-controls.json',D/'validation/epoch3/current-negative-controls.json');shutil.copy2(Path(__file__),D/'validation/epoch3/stress-continue-epoch3.py')
