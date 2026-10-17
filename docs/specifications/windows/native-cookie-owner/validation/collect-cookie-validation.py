"""Collect complete core, embedding and failed-attempt evidence by explicit name."""
from pathlib import Path
import datetime, hashlib, json
BASE=Path(__file__).resolve().parent
SHARE=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
DEST=BASE/'rax-final-source/docs/specifications/windows/native-cookie-owner/validation'
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def need(x,m):
 if not x:raise ValueError(m)
def main():
 need(__debug__,'optimized collection')
 need(load(SHARE/'cookie-owner-final-owning-restored.json').get('exit-code')==0,'native owning recovery is incomplete')
 copies={}
 def add(p,name=None):
  name=name or p.name
  need(p.is_file() and name not in copies,'missing or duplicate '+str(p))
  copies[name]=p
 for host in ['macos','linux']:
  for phase,prefix in [('initial',host+'-cookie'),('final',host+'-cookie-final')]:
   gates=load(BASE/(prefix+'-gates.json'))
   need(len(gates)==4,'four core gates')
   need(all(x['exit-code']==(101 if phase=='initial' and x['gate']=='lib' else 0) for x in gates),'core results')
   add(BASE/(prefix+'-gates.json'),host+'/'+phase+'/gates.json')
   for row in gates:add(BASE/row['log'],host+'/'+phase+'/'+row['log'])
  gates=load(BASE/(host+'-owning-gates.json'))
  need(len(gates)==(6 if host=='macos' else 4) and all(x['exit-code']==0 for x in gates),'owning results')
  add(BASE/(host+'-owning-gates.json'))
  for row in gates:add(BASE/row['log'])
  for name in ['owning-commands.json','final-owning-archive-identities.json']:add(BASE/(host+'-'+name))
 for name in ['final-code-inputs.json','final-owning-source-hashes.json','run-cookie-unix-gates.py','run-cookie-final-unix.py','run-owning-gates.py','collect-cookie-archives.py','owning-linux-cpp-build.py','collect-cookie-validation.py','initial-scope.json','macos-focused-initial.log','linux-focused-initial.log','linux-focused-initial-result.json']:
  add(BASE/name)
 for phase,tree in [('initial','rax-source'),('final','rax-final-source')]:
  for name in ['src/user/windows/loader/mod.rs','src/user/windows/loader/cookie_tests.rs']:
   add(BASE/tree/name,'compiled-source/'+phase+'/'+name)
 for folder in ['interrupted-final-attempt-20261017','windows-initial-owning-failure','windows-first-recovery-failure']:
  for p in sorted((BASE/folder).iterdir()):
   need(p.is_file(),'unexpected nested evidence');add(p,folder+'/'+p.name)
 gates=load(SHARE/'native-cookie-owner-final-verified-gates.json')['gates']
 need(len(gates)==6 and all(x['exit-code']==(101 if x['gate']=='full' else 0) for x in gates),'native core results')
 names=['cookie-owner-final-launch.json','cookie-owner-final-core-driver.log','native-cookie-owner-final-verified-gates.json','native-cookie-owner-final-gates.json','native-cookie-owner-final-gates.ps1','run-cookie-owner-final-atomic.py','apply-cookie-owner-final.py','cookie-owner-final-overlay.json','native-cookie-owner-final-source-verification.json','native-cookie-owner-final-build-user.log','native-cookie-owner-final-observer-build.log','run-cookie-owner-final-observations.py','native-cookie-owner-final-ordinary-outcomes.json','cookie-owner-final-owning-driver.log','native-cookie-owner-final-owning-source-identities.json','native-cookie-owner-final-owning-source-verification.json','assist-native-cookie-owner-final-owned.ps1','cookie-owner-final-owning-source-hashes.json','verify-cookie-owner-final-owning-sources.py','inspect-cookie-owner-final-archives.py','cookie-owner-final-owning-restored.json','cookie-owner-final-restored-driver.log','run-cookie-owning-restored.py','assist-native-cookie-owner-final-restored-owned.ps1','assist-native-cookie-owner-final-restored-shipping.log','assist-native-cookie-owner-final-restored-cpp.log','native-cookie-owner-final-restored-after-artifacts.jsonl','native-cookie-owner-final-restored-after-artifacts-stderr.log','native-cookie-owner-final-restored-owning-source-identities.json','native-cookie-owner-final-restored-owning-source-verification.json','native-cookie-owner-final-restored-owning-archive-hashes.json','verify-cookie-owner-final-restored-owning-sources.py','inspect-cookie-owner-final-restored-archives.py','cookie-owner-final-restored-owning-source-hashes.json','cookie-dependency-restore-inputs.json','cookie-dependency-missing.json','inspect-cookie-dependencies.py','inspect-cookie-dependencies2.py','verify-current-cpp-consumers.py','current-cpp-consumers-reverified.json','native-secure-capture-current-cpp-verification.json']
 names += [x['log'] for x in gates]
 names += [x['log'] for x in load(SHARE/'native-cookie-owner-final-ordinary-outcomes.json') if 'log' in x]
 for name in names:add(SHARE/name,'windows/'+name)
 for name in ['owning-validation-boundary.json','latest-published-cpp-consumer-identities.json']:
  add(BASE.parent/'native-create-file-20261015'/name)
 rows=[]
 for name,p in sorted(copies.items()):
  data=p.read_bytes();out=DEST/name;out.parent.mkdir(parents=True,exist_ok=True)
  if out.exists():need(out.read_bytes()==data,'refuse rewriting earlier evidence '+name)
  else:out.write_bytes(data)
  rows.append({'path':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
 (DEST/'results.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'parent':'8b038b528d8cd88e44e5ebd672ef14e365c480a5','selected-owning-inputs':310,'files':rows},indent=2)+'\n')
 print('Collected',len(rows),'files;',sum(x['bytes'] for x in rows),'bytes')
if __name__=='__main__':main()
