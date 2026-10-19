from pathlib import Path
import hashlib,json,runpy,shutil,tempfile
Q=Path(__file__).parent;R=Path('/Users/int/.assist-continue-20261019');D=R/'docs/specifications/windows/native-continue'
def wrong_reason(root):
 p=root/'validation/continue-owning-capi-b.json';d=json.loads(p.read_text());d['outcomes'][0]['reason']=1;row=d['steps'][1];f=root/'validation'/row['log'];lines=f.read_text().splitlines();changed=False
 for i,line in enumerate(lines):
  if line.startswith('api-result ')and not changed:
   value=json.loads(line[len('api-result '):]);value['reason']=1;lines[i]='api-result '+json.dumps(value);changed=True
 assert changed;f.write_text('\n'.join(lines)+'\n');raw=f.read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest());p.write_text(json.dumps(d,indent=2)+'\n')
def carrier(root):
 p=root/'validation/continue-owning-capi-b.json';d=json.loads(p.read_text());d['carrier-after']['sha256']='0'*64;p.write_text(json.dumps(d,indent=2)+'\n')
runpy.run_path(str(D/'check_capi.py'))['check']();controls=[]
with tempfile.TemporaryDirectory(prefix='continue-capi-controls-',dir=R/'target')as temporary:
 for name,mutate in[('capi-wrong-smoke-exit-reason',wrong_reason),('capi-carrier-drift',carrier)]:
  root=Path(temporary)/name;shutil.copytree(D,root);mutate(root)
  try:runpy.run_path(str(root/'check_capi.py'))['check']()
  except(AssertionError,KeyError,ValueError)as error:controls.append({'name':name,'rejected':True,'error-type':type(error).__name__})
  else:raise AssertionError('Accepted corrupted C API proof '+name)
  shutil.rmtree(root);print('Rejected',name,flush=True)
(Q/'continue-capi-negative-controls.json').write_text(json.dumps({'controls':controls,'all-rejected':True,'scope':'Independent C API proof checker; ordinary API record and log hashes rebound when changed.'},indent=2)+'\n')
