"""Collect completed source-bound WNF query validation without replacing proof."""
from pathlib import Path
import datetime,hashlib,json
B=Path(__file__).resolve().parent
S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D=B/'rax-source/docs/specifications/windows/native-wnf-query/validation'
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def need(v,m):
 if not v:raise ValueError(m)
def main():
 need(__debug__,'optimized collection rejected')
 done=load(S/'wnf-query-final-retry-launch.json');need(done.get('state')=='finished'and done.get('exit-code')==0,'native final driver incomplete')
 files={}
 def add(p,n=None):
  n=n or p.name;need(p.is_file()and n not in files,'missing/duplicate '+str(p));files[n]=p
 for host in ['macos','linux']:
  for epoch,count in [('epoch2',4),('epoch3',4),('final',4)]:
   p=B/(host+'-wnf-'+epoch+'-gates.json');g=load(p);need(len(g)==count and all(x['exit-code']==0 for x in g),'complete '+p.name);add(p)
   for x in g:add(B/x['log'])
  initial=B/(host+'-wnf-initial-gates.json');need(load(initial)[0]['exit-code']==101,'retain initial compile failure');add(initial)
  for p in sorted(B.glob(host+'-wnf-initial-*.log')):add(p)
  gates=load(B/(host+'-owning-gates.json'));need(len(gates)==(6 if host=='macos'else 4) and all(x['exit-code']==0 for x in gates),'complete owning '+host)
  for name in [host+'-owning-gates.json',host+'-owning-commands.json',host+'-final-owning-archive-identities.json',*[x['log']for x in gates]]:add(B/name)
 for name in ['source-baseline.json','owned-source-plan.json','initial-source-inputs.json','epoch2-source-inputs.json','epoch3-source-inputs.json','initial-owning-source-hashes.json','epoch2-owning-source-hashes.json','epoch3-owning-source-hashes.json','final-owning-source-hashes.json','initial-unix-retries-stopped.json','initial-review-notes.json','run-wnf-initial-unix.py','run-wnf-epoch2-unix.py','run-wnf-epoch3-unix.py','run-wnf-final-unix.py','run-owning-gates.py','owning-linux-cpp-build.py','collect-wnf-archives.py','collect-wnf-validation.py','apply-local-epoch2.py','scope-replay-append.rs','owning-carrier-repoint.json']:
  add(B/name)
 for epoch,manifest,folder in [('initial','initial-source-inputs.json',B/'source-epochs/initial'),('epoch2','epoch2-source-inputs.json',B/'source-epochs/epoch2'),('final','epoch3-source-inputs.json',B/'rax-source')]:
  for row in load(B/manifest)['files']:add(folder/row['path'],'compiled-source/'+epoch+'/'+row['path'])
 native_names=['wnf-query-initial-launch.json','wnf-query-initial-driver.log','wnf-query-initial-stop.json','stop-wnf-initial-retries.py','apply-wnf-query-initial.py','native-wnf-query-initial-gates.ps1','native-wnf-query-initial-gates.json','native-wnf-query-initial-source-verification.json','run-wnf-query-initial-atomic.py',
 'wnf-query-epoch2-launch.json','wnf-query-epoch2-driver.log','apply-wnf-query-epoch2.py','native-wnf-query-epoch2-gates.ps1','native-wnf-query-epoch2-gates.json','native-wnf-query-epoch2-source-verification.json','run-wnf-query-epoch2-atomic.py',
 'wnf-query-epoch3-launch.json','wnf-query-epoch3-driver.log','apply-wnf-query-epoch3.py','native-wnf-query-epoch3-gates.ps1','native-wnf-query-epoch3-source-verification.json','run-wnf-query-epoch3-atomic.py',
 'wnf-query-final-launch.json','wnf-query-final-epoch3-driver.log','run-wnf-query-chain.py',
 'wnf-query-epoch3-retry-launch.json','wnf-query-epoch3-retry-driver.log','verify-wnf-query-epoch3-retry.py','native-wnf-query-epoch3-retry-gates.ps1','native-wnf-query-epoch3-retry-gates.json','run-wnf-query-epoch3-retry-atomic.py',
 'wnf-query-final-retry-launch.json','wnf-query-final-retry-epoch3-driver.log','wnf-query-final-retry-core-driver.log','wnf-query-final-retry-owning-driver.log','run-wnf-query-final-retry-chain.py','native-wnf-query-final-gates.ps1','native-wnf-query-final-gates.json','native-wnf-query-final-verified-gates.json','native-wnf-query-final-source-verification.json','verify-wnf-query-current.py',
 'native-wnf-query-final-build-user.log','native-wnf-query-final-build-user-artifacts.jsonl','native-wnf-query-final-observer-build.log','run-wnf-query-final-observations.py','native-wnf-query-final-ordinary-outcomes.json',
 'assist-native-wnf-query-final-owned.ps1','verify-wnf-query-final-owning-sources.py','inspect-wnf-query-final-archives.py','assist-native-wnf-query-final-shipping.log','assist-native-wnf-query-final-cpp.log','native-wnf-query-final-after-artifacts.jsonl','native-wnf-query-final-after-artifacts-stderr.log','native-wnf-query-final-owning-source-identities.json','native-wnf-query-final-owning-source-verification.json','native-wnf-query-final-owning-archive-hashes.json','wnf-query-final-owning-source-hashes.json','current-cpp-consumers-reverified.json','native-secure-capture-current-cpp-verification.json']
 for epoch in ['initial','epoch2','epoch3-retry','final']:
  g=load(S/('native-wnf-query-'+epoch+'-gates.json'));g=g if isinstance(g,list)else[g]
  native_names.extend(x['log']for x in g)
 native_names.extend(x['log']for x in load(S/'native-wnf-query-final-ordinary-outcomes.json')if'log'in x)
 for name in native_names:add(S/name,'windows/'+name)
 for name in ['owning-validation-boundary.json','latest-published-cpp-consumer-identities.json']:add(B.parent/'native-create-file-20261015'/name)
 rows=[]
 for name,p in sorted(files.items()):
  target=D/name;target.parent.mkdir(parents=True,exist_ok=True);raw=p.read_bytes()
  if target.exists():need(target.read_bytes()==raw,'refuse evidence replacement '+name)
  else:target.write_bytes(raw)
  rows.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
 (D/'results.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'parent':'7279261c3ab518e0d0b027817cabf776e9700eb8','selected-owning-inputs':326,'files':rows},indent=2)+'\n')
 print('Collected',len(rows),'validation records;',sum(x['bytes']for x in rows),'bytes')
if __name__=='__main__':main()
