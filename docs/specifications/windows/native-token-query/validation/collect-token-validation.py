"""Collect completed token evidence while preserving the Windows-only test fix."""
from pathlib import Path
import datetime,hashlib,json
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=B/'rax-final-source/docs/specifications/windows/native-token-query/validation'
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def need(x,m):
 if not x:raise ValueError(m)
def main():
 need(__debug__,'optimized collector')
 native=load(S/'token-query-epoch2-launch.json');need(native.get('state')=='finished' and native.get('exit-code')==0,'native epoch2 incomplete')
 files={}
 def add(p,n=None):
  n=n or p.name;need(p.is_file() and n not in files,'missing or duplicate '+str(p));files[n]=p
 for h in ['macos','linux']:
  for epoch,prefix,count in [(1,'token-final',7),(2,'token-epoch2',4)]:
   gates=load(B/(h+'-'+prefix+'-gates.json'));need(len(gates)==count and all(g['exit-code']==0 for g in gates),'incomplete core '+h)
   add(B/(h+'-'+prefix+'-gates.json'))
   for g in gates:add(B/g['log'])
  for folder in [B/(h+'-owning-epoch1'),B]:
   gates=load(folder/(h+'-owning-gates.json'));need(len(gates)==(6 if h=='macos' else 4) and all(g['exit-code']==0 for g in gates),'incomplete owning '+h)
   for n in [h+'-owning-gates.json',h+'-owning-commands.json',h+'-final-owning-archive-identities.json',*[g['log'] for g in gates]]:
    add(folder/n,(h+'-owning-epoch1/'+n) if folder!=B else n)
 for n in ['epoch1-code-inputs.json','epoch2-code-inputs.json','epoch2-owning-source-hashes.json','epoch2-change.json','unix-source-equivalence.json','preliminary-token-code.json','preliminary-token-v2-code.json','token-scope.json','macos-token-preliminary-v1-compile-failure.log','macos-token-preliminary-v1-compile-failure-result.json','macos-token-preliminary-v2.log','macos-token-preliminary-v2-result.json','prepare-token-final.py','prepare-token-epoch2-native.py','run-token-final-unix.py','run-token-epoch2-unix.py','run-owning-gates.py','owning-linux-cpp-build.py','collect-token-archives.py','collect-token-validation.py']:add(B/n)
 for folder in ['epoch1-link-failure','epoch2-token-open-failure']:
  for p in sorted((B/folder).iterdir()):add(p,folder+'/'+p.name)
 for epoch,tree in [(1,'rax-source'),(2,'rax-final-source')]:
  for row in load(B/f'epoch{epoch}-code-inputs.json')['files']:add(B/tree/row['path'],f'compiled-source/epoch{epoch}/'+row['path'])
 names=['token-query-final-launch.json','token-query-final-core-driver.log','token-query-final-overlay.json','token-query-final-owning-source-hashes.json','run-token-query-final-atomic.py','run-token-query-final-observations.py','apply-token-query-final.py','native-token-query-final-gates.ps1','native-token-query-final-gates.json','native-token-query-final-source-verification.json','native-token-query-final-build-user.log','native-token-query-final-build-user-artifacts.jsonl','native-token-query-final-observer-build.log','native-token-query-final-ordinary-outcomes.json','token-query-epoch2-launch.json','token-query-epoch2-core-driver.log','token-query-epoch2-owning-driver.log','token-query-epoch2-delta.json','token-query-epoch2-owning-source-hashes.json','run-token-query-epoch2-atomic.py','apply-token-query-epoch2.py','native-token-query-epoch2-gates.ps1','native-token-query-epoch2-gates.json','native-token-query-epoch2-verified-gates.json','native-token-query-epoch2-source-verification.json','assist-native-token-query-epoch2-owned.ps1','verify-token-query-epoch2-owning-sources.py','inspect-token-query-epoch2-archives.py','assist-native-token-query-epoch2-shipping.log','assist-native-token-query-epoch2-cpp.log','native-token-query-epoch2-after-artifacts.jsonl','native-token-query-epoch2-after-artifacts-stderr.log','native-token-query-epoch2-owning-source-identities.json','native-token-query-epoch2-owning-source-verification.json','native-token-query-epoch2-owning-archive-hashes.json','current-cpp-consumers-reverified.json','native-secure-capture-current-cpp-verification.json']
 for prefix in ['final','epoch2']:
  gates=load(S/f'native-token-query-{prefix}-gates.json');gates=gates if isinstance(gates,list) else [gates]
  names += [g['log'] for g in gates]
 names += [x['log'] for x in load(S/'native-token-query-final-ordinary-outcomes.json') if 'log' in x]
 for n in names:add(S/n,'windows/'+n)
 for n in ['owning-validation-boundary.json','latest-published-cpp-consumer-identities.json']:add(B.parent/'native-create-file-20261015'/n)
 rows=[]
 for n,p in sorted(files.items()):
  data=p.read_bytes();out=D/n;out.parent.mkdir(parents=True,exist_ok=True)
  if out.exists():need(out.read_bytes()==data,'do not replace evidence '+n)
  else:out.write_bytes(data)
  rows.append({'path':n,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
 (D/'results.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'parent':'aa9bcc5651164ade3265e124de9a6d4f778f561f','selected-owning-inputs':313,'files':rows},indent=2)+'\n')
 print('Collected',len(rows),'files;',sum(x['bytes'] for x in rows),'bytes')
if __name__=='__main__':main()
