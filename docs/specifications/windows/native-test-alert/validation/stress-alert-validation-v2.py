from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
P=Path(__file__).parent;R=Path('/Users/int/.assist-alert3-20261019');D=R/'docs/specifications/windows/native-test-alert'
def edit(root,name,fn):
 p=root/'validation'/name;d=json.loads(p.read_text());fn(d);p.write_text(json.dumps(d,indent=2)+'\n')
def log_change(root,record,name,old,new):
 p=root/'validation'/record;d=json.loads(p.read_text());row=next(x for x in d['steps']if x.get('name',x.get('gate'))==name);log=root/'validation'/row['log'];text=log.read_text();changed=text.replace(old,new);assert changed!=text;log.write_text(changed);raw=log.read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest());row['results']=re.findall(r'test result: ([^\r\n]+)',changed)
 if record=='ordinary.json':
  for outcome in d['outcomes']:
   if outcome['log']==log.name:outcome['log-sha256']=row['sha256']
 p.write_text(json.dumps(d,indent=2)+'\n')
def wrong_windows(root):
 old='smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime';new=old+'_unrecognized'
 log_change(root,'gates-windows.json','full',old,new)
 edit(root,'gates-windows.json',lambda d:d.update({'exact-existing-failures':[new if x==old else x for x in d['exact-existing-failures']]}))
def wrong_native_control(root):
 def change(d):
  for suffix in ['before','after']:d['outcomes'][0]['native-'+suffix]['exit-code']=1
 edit(root,'ordinary.json',change)
def false_success(root):
 p=root/'validation/ordinary.json';d=json.loads(p.read_text());row=next(x for x in d['steps']if x['name']=='smoke');log=root/'validation'/row['log'];text=log.read_text();old=next(x for x in text.splitlines()if x.startswith('terminal '));changed=text.replace(old,'terminal turn=2943204 Complete(Exited(0))');log.write_text(changed);raw=log.read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest());d['outcomes'][0]['log-sha256']=row['sha256'];p.write_text(json.dumps(d,indent=2)+'\n');edit(root,'summary.json',lambda x:x['ordinary-programs']['outcomes'][0].update({'terminal':'Complete(Exited(0))'}))
def drop_assumptions(root):
 p=root/'validation/final-assumptions.json';p.unlink();m=root/'evidence-manifest.json';d=json.loads(m.read_text());d['files']=[x for x in d['files']if x['path']!='validation/final-assumptions.json'];m.write_text(json.dumps(d)+'\n')
def bind(root):
 p=root/'evidence-manifest.json';d=json.loads(p.read_text())
 for row in d['files']:
  raw=(root/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 p.write_text(json.dumps(d,indent=2)+'\n')
controls=[
 ('summary-library-count',lambda r:edit(r,'summary.json',lambda d:d['library-gates']['macos'].update(passed=8056))),
 ('coherent-library-count',lambda r:log_change(r,'gates-macos.json','full','8057 passed;','8056 passed;')),
 ('filtered-full-selection',lambda r:log_change(r,'gates-linux.json','full','0 filtered out;','1 filtered out;')),
 ('unrecognized-windows-failure',wrong_windows),
 ('capi-count',lambda r:log_change(r,'gates-linux.json','capi','183 passed;','182 passed;')),
 ('integration-count',lambda r:log_change(r,'gates-macos.json','integration','544 passed;','543 passed;')),
 ('core-feature-set',lambda r:edit(r,'archives-windows-before.json',lambda d:d.update({'core-features':['smir-jit']}))),
 ('archive-drift',lambda r:edit(r,'archives-windows-after.json',lambda d:d['archives'][1].update(sha256='0'*64))),
 ('compiled-helper-input',lambda r:edit(r,'owning-source-hashes.json',lambda d:d['helpers'][0].update(sha256='0'*64))),
 ('ordinary-core-drift',lambda r:edit(r,'ordinary.json',lambda d:d['core-after'].update(sha256='0'*64))),
 ('native-control-failure',wrong_native_control),
 ('false-guest-success',false_success),
 ('alert-return-status',lambda r:log_change(r,'ordinary.json','smoke','status=0x0','status=0x101')),
 ('false-full-objective',lambda r:edit(r,'summary.json',lambda d:d.update({'full-process-goal-complete':True}))),
 ('missing-assumption-register',drop_assumptions),
 ('false-negative-control-receipt',lambda r:edit(r,'alert-final-negative-controls.json',lambda d:d.update({'all-rejected':False}))),
 ('false-whole-root-products',lambda r:edit(r,'summary.json',lambda d:d.update({'whole-current-root-products-validated':True}))),
]
runpy.run_path(str(D/'check_validation.py'))['check']('macos',R);results=[]
with tempfile.TemporaryDirectory(prefix='alert-final-controls-',dir=R/'target')as temporary:
 for name,mutate in controls:
  root=Path(temporary)/name;shutil.copytree(D,root);mutate(root);bind(root)
  try:runpy.run_path(str(root/'check_validation.py'))['check']('macos',R)
  except (AssertionError,KeyError,ValueError,FileNotFoundError)as error:results.append({'name':name,'rejected':True,'error-type':type(error).__name__})
  else:raise AssertionError('Accepted corruption '+name)
  shutil.rmtree(root);print('Rejected',name,flush=True)
(P/'alert-final-negative-controls-v2.json').write_text(json.dumps({'controls':results,'all-rejected':True,'scope':'Outer envelope rebound after each corruption; log/record hashes also rebound when applicable. Product source and proof checker unchanged.'},indent=2)+'\n')
