from pathlib import Path
import hashlib,json,datetime
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=B/'rax-source/docs/specifications/windows/native-wnf-system-subscribe/validation'
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def need(v,m):
 if not v:raise ValueError(m)
def main():
 r=load(S/'wnf-system-final-launch.json');need(r.get('state')=='finished'and r.get('exit-code')==0,'native chain incomplete')
 # The waiting controller's first version appends a literal backslash-n.
 # Preserve that exact host receipt, then encode the unchanged fields as JSON.
 receipt=B/'completed-artifact-copy.json';raw=receipt.read_bytes()
 try:json.loads(raw)
 except json.JSONDecodeError:
  need(raw.endswith(b'\\n'),'unexpected controller receipt format')
  value=json.loads(raw[:-2]);(B/'completed-artifact-copy-original.txt').write_bytes(raw)
  receipt.write_text(json.dumps(value,indent=2)+'\n')
 files={}
 def add(p,n=None):
  n=n or p.name;need(p.is_file()and n not in files,'missing/duplicate '+str(p));files[n]=p
 for host in ['macos','linux']:
  for stage,count in [('wnf-system-initial',3),('wnf-system-epoch2',3),('wnf-system-final',4),('system-owning',6 if host=='macos'else 4)]:
   p=B/(host+'-'+stage+'-gates.json');rows=load(p);need(len(rows)==count,'incomplete gates '+p.name)
   if stage!='wnf-system-initial':need(all(x['exit-code']==0 for x in rows),'failed '+p.name)
   add(p)
   for x in rows:add(B/x['log'])
  for n in [host+'-system-owning-commands.json',host+'-system-final-owning-archive-identities.json']:add(B/n)
 for n in ['source-baseline.json','owned-source-plan.json','initial-source-inputs.json','epoch2-source-inputs.json','initial-owning-source-hashes.json','epoch2-owning-source-hashes.json','final-owning-source-hashes.json','epoch2-correction.json','owning-carrier-repoint.json','run-system-initial-unix.py','run-system-epoch2-unix.py','run-system-final-unix.py','run-system-owning-gates.py','after-focus.py','owning-linux-cpp-build.py','collect-system-archives.py','collect-system-validation.py','native-initial-controller.json','native-epoch2-controller.json','run-system-final-unix-original.py','full-waiter-correction.json','collect-system-when-complete.py','completed-artifact-copy.json','ordinary-continuation.json','pre-publication-review.json']:
  add(B/n)
 for label,m,root in [('initial','initial-source-inputs.json',B/'source-epochs/initial'),('final','epoch2-source-inputs.json',B/'rax-source')]:
  for x in load(B/m)['files']:add(root/x['path'],'compiled-source/'+label+'/'+x['path'])
 prefixes=['wnf-system-','native-wnf-system-','assist-native-wnf-system-','apply-system-','verify-system-','verify-wnf-system-','inspect-wnf-system-','run-wnf-system-']
 for p in sorted(S.iterdir()):
  if p.is_file()and p.suffix in ['.py','.ps1','.json','.jsonl','.log']and any(p.name.startswith(x)for x in prefixes):add(p,'windows/'+p.name)
 for n in ['owning-validation-boundary.json','latest-published-cpp-consumer-identities.json']:add(B.parent/'native-create-file-20261015'/n)
 for n in ['current-cpp-consumers-reverified.json','native-secure-capture-current-cpp-verification.json']:add(S/n,'windows/'+n)
 if (B/'completed-artifact-copy-original.txt').exists():add(B/'completed-artifact-copy-original.txt')
 rows=[]
 for name,p in sorted(files.items()):
  q=D/name;q.parent.mkdir(parents=True,exist_ok=True);raw=p.read_bytes()
  if q.exists():need(q.read_bytes()==raw,'refuse replacing retained record '+name)
  else:q.write_bytes(raw)
  rows.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
 (D/'results.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'base':'44d21496b9c81e3911e7bbb4160eb13b22ca183d','selected-owning-inputs':332,'files':rows},indent=2)+'\n')
 print('Collected',len(rows),'System WNF validation records',flush=True)
if __name__=='__main__':main()
