"""Reject coherent corrupted claims after log/receipt/envelope rebinding."""
from pathlib import Path
import hashlib,importlib.util,json,shutil,tempfile
ROOT=Path('/Users/int/.assist-session-alias-20261020');CACHE=Path(__file__).parent;DOCS=ROOT/'docs/specifications/windows/native-session-directory-alias'
def load(p):return json.loads(p.read_text())
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def seal(d):
 rows=[{'path':p.relative_to(d).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}for p in sorted(d.rglob('*'))if p.is_file()and p.name!='evidence-manifest.json'for raw in[p.read_bytes()]];write(d/'evidence-manifest.json',{'files':rows})
spec=importlib.util.spec_from_file_location('alias_proof',DOCS/'check_validation.py');checker=importlib.util.module_from_spec(spec);spec.loader.exec_module(checker);original_identity=checker.identity;immutable={}
def identity(path):
 if path.is_relative_to(ROOT)and not path.is_relative_to(DOCS)and not path.is_relative_to(ROOT/'target'):
  if path not in immutable:immutable[path]=original_identity(path)
  return immutable[path]
 return original_identity(path)
checker.identity=identity;checker.check('macos',ROOT);results=[]
with tempfile.TemporaryDirectory(prefix='alias-negative-',dir=ROOT/'target')as directory:
 trial=Path(directory)/'proof';shutil.copytree(DOCS,trial);checker.DOCS=trial;checker.VALIDATION=trial/'validation';saved={p.relative_to(trial):p.read_bytes()for p in trial.rglob('*')if p.is_file()}
 def control(name,mutate):
  for rel,raw in saved.items():(trial/rel).write_bytes(raw)
  seal(trial);checker.check('macos',ROOT)
  mutate(trial);seal(trial)
  try:checker.check('macos',ROOT)
  except AssertionError:results.append({'name':name,'rejected':True})
  else:raise AssertionError('accepted '+name)
 def change(d,name,mutate):
  p=d/'validation'/name;v=load(p);mutate(v);write(p,v)
 def native(d,mutate):
  p=d/'validation/native-session-alias-v1-launch.json';v=load(p);row=next(r for r in v['steps']if r['name']=='arm64');log=d/'validation'/row['log'];raw=mutate(log.read_text()).encode();log.write_bytes(raw);row.update({'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()});write(p,v)
 control('same-object-evidence-reversed',lambda d:native(d,lambda s:s.replace('identity same=00000000','identity same=C00001AC')))
 control('distinct-object-falsely-equal',lambda d:native(d,lambda s:s.replace('different=C00001AC','different=00000000')))
 control('dont-reparse-falsely-opens',lambda d:native(d,lambda s:s.replace('dont-reparse status=C000050B opened=0','dont-reparse status=00000000 opened=1')))
 control('symbolic-link-type-lost',lambda d:native(d,lambda s:s.replace('type=530079006D0062006F006C00690063004C0069006E006B00','type=4400690072006500630074006F0072007900')))
 control('private-handle-leak',lambda d:native(d,lambda s:s.replace('handle-count-after=43','handle-count-after=44')))
 control('frozen-source-identity',lambda d:change(d,'session-alias-source-epoch1.json',lambda v:v['files'][0].update({'sha256':'0'*64})))
 control('new-windows-failure',lambda d:change(d,'gates-windows.json',lambda v:v['exact-existing-failures'].append('new::failure')))
 def features(d):
  for name in ['archives-windows-before.json','archives-windows-after.json']:change(d,name,lambda v:v.update({'core-features':['jit']}))
 control('coherent-core-feature-drift',features)
 def wrongcore(d):
  def mutate(v):
   for key in ['core-before','core-after']:v[key]['sha256']='0'*64
  change(d,'ordinary.json',mutate)
 control('coherent-observer-core-substitution',wrongcore)
 control('capi-failure-as-exit',lambda d:change(d,'capi.json',lambda v:v['outcomes'][1].update({'reason':4})))
 def baseline(d):
  p=d/'validation/baseline-source/src/user/windows/object_namespace.rs';p.write_bytes(p.read_bytes()+b'\n')
 control('baseline-source-substitution',baseline)
 control('invented-current-ci-target',lambda d:change(d,'summary.json',lambda v:v.update({'ci-session-alias-target':'verified-global'})))
 control('false-whole-goal-completion',lambda d:change(d,'summary.json',lambda v:v.update({'full-process-goal-complete':True})))
value={'all-rejected':True,'controls':results,'scope':'Each relevant log/receipt and enclosing byte ledger is rebound before semantic rejection. Immutable product/source identities are cached only during this stress runner.'};value['positive-restorations-verified']=len(results);write(Path('/Users/int/Library/Caches/assist-process-emulation/native-query-attributes-20261020/file-attributes-prior-alias-stress-revalidation-epoch1.json'),value);print(json.dumps(value,indent=2))
