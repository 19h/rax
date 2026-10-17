"""Collect final corrected WNF proof and retain superseded source epochs."""
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
 done=load(S/'wnf-query-final4-launch.json');need(done.get('state')=='finished'and done.get('exit-code')==0,'native final4 incomplete')
 files={}
 def add(p,n=None):
  n=n or p.name;need(p.is_file()and n not in files,'missing/duplicate '+str(p));files[n]=p
 for host in ['macos','linux']:
  for epoch in ['epoch2','epoch3','epoch4','final4']:
   p=B/(host+'-wnf-'+epoch+'-gates.json');g=load(p);need(len(g)==4 and all(x['exit-code']==0 for x in g),'complete '+p.name);add(p)
   for x in g:add(B/x['log'])
  p=B/(host+'-wnf-initial-gates.json');need(load(p)[0]['exit-code']==101,'initial compile failure');add(p)
  for p in sorted(B.glob(host+'-wnf-initial-*.log')):add(p)
  gates=load(B/(host+'-owning4-gates.json'));need(len(gates)==(6 if host=='macos'else 4)and all(x['exit-code']==0 for x in gates),'complete owning4 '+host)
  for name in [host+'-owning4-gates.json',host+'-owning4-commands.json',host+'-final4-owning-archive-identities.json',*[x['log']for x in gates]]:add(B/name)
 for p in sorted((B/'epoch3-validation').rglob('*')):
  if p.is_file():add(p,'superseded-epoch3/'+str(p.relative_to(B/'epoch3-validation')))
 names=['source-baseline.json','owned-source-plan.json','initial-source-inputs.json','epoch2-source-inputs.json','epoch3-source-inputs.json','epoch4-source-inputs.json','initial-owning-source-hashes.json','epoch2-owning-source-hashes.json','epoch3-owning-source-hashes.json','epoch4-owning-source-hashes.json','final-owning-source-hashes.json','initial-unix-retries-stopped.json','initial-review-notes.json','epoch3-validation-stopped.json','epoch4-scope-falsification.json','run-wnf-initial-unix.py','run-wnf-epoch2-unix.py','run-wnf-epoch3-unix.py','run-wnf-epoch4-unix.py','run-wnf-final4-unix.py','run-owning4-gates.py','run-owning4-after-focus.py','owning-linux-cpp-build.py','collect-wnf-archives.py','collect-wnf-validation.py','collect-wnf-validation-epoch3.py','apply-local-epoch2.py','apply-local-epoch4.py','scope-replay-append.rs','owning-carrier-repoint.json']
 for n in names:add(B/n)
 for label,name,folder in [('initial','initial-source-inputs.json',B/'source-epochs/initial'),('epoch2','epoch2-source-inputs.json',B/'source-epochs/epoch2'),('epoch3','epoch3-source-inputs.json',B/'source-epochs/epoch3'),('final','epoch4-source-inputs.json',B/'rax-source')]:
  for row in load(B/name)['files']:add(folder/row['path'],'compiled-source/'+label+'/'+row['path'])
 required=['wnf-query-final4-launch.json','wnf-query-final4-core-driver.log','wnf-query-final4-owning-driver.log','native-wnf-query-final4-gates.json','native-wnf-query-final4-verified-gates.json','native-wnf-query-final4-full.log','native-wnf-query-final4-source-verification.json','native-wnf-query-final4-owning-archive-hashes.json','native-wnf-query-final4-after-artifacts.jsonl','native-wnf-query-final4-after-artifacts-stderr.log','native-wnf-query-final4-build-user-artifacts.jsonl','native-wnf-query-final4-ordinary-outcomes.json']
 for n in required:need((S/n).is_file(),'required final4 native record '+n)
 prefixes=['wnf-query-initial-','wnf-query-epoch2-','wnf-query-epoch3-','wnf-query-epoch4-','wnf-query-final-','wnf-query-final4-','wnf-epoch3-final-','native-wnf-query-initial-','native-wnf-query-epoch2-','native-wnf-query-epoch3-','native-wnf-query-epoch4-','native-wnf-query-final-','native-wnf-query-final4-','apply-wnf-query-','run-wnf-query-','verify-wnf-query-','inspect-wnf-query-','assist-native-wnf-query-final']
 for p in sorted(S.iterdir()):
  if p.is_file()and p.suffix in ['.json','.jsonl','.log','.py','.ps1']and any(p.name.startswith(x)for x in prefixes):add(p,'windows/'+p.name)
 for n in ['stop-wnf-initial-retries.py','stop-wnf-epoch3-final.py','current-cpp-consumers-reverified.json','native-secure-capture-current-cpp-verification.json']:add(S/n,'windows/'+n)
 for n in ['owning-validation-boundary.json','latest-published-cpp-consumer-identities.json']:add(B.parent/'native-create-file-20261015'/n)
 rows=[]
 for name,p in sorted(files.items()):
  target=D/name;target.parent.mkdir(parents=True,exist_ok=True);raw=p.read_bytes()
  if target.exists():need(target.read_bytes()==raw,'refuse evidence replacement '+name)
  else:target.write_bytes(raw)
  rows.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
 (D/'results.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'parent':'7279261c3ab518e0d0b027817cabf776e9700eb8','selected-owning-inputs':327,'files':rows},indent=2)+'\n')
 print('Collected',len(rows),'records;',sum(x['bytes']for x in rows),'bytes')
if __name__=='__main__':main()
