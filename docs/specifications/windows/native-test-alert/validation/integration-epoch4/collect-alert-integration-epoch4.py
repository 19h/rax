from pathlib import Path
import datetime,hashlib,json,re,shutil
P=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('/Users/int/.assist-alert3-20261019');D=R/'docs/specifications/windows/native-test-alert';V=D/'validation/integration-epoch4';V.mkdir(exist_ok=True)
assert not(V/'summary.json').exists()
def load(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def copy(source,name=None):
 target=V/(name or source.name)
 if target.exists():assert target.read_bytes()==source.read_bytes(),str(target)
 else:shutil.copy2(source,target)
 return target
def receipt(root,name,target):
 path=root/name;record=load(path);assert record['state']=='finished'and record['exit-code']==0,name
 copy(path,target)
 for row in record['steps']:
  log=root/row['log'];raw=log.read_bytes();assert len(raw)==row['bytes']and hashlib.sha256(raw).hexdigest()==row['sha256'],str(log);copy(log)
 return record
gates={}
for host in ['macos','linux','windows']:
 root=S if host=='windows'else P
 gates[host]=receipt(root,'alert-native-epoch4-gates.json'if host=='windows'else'alert-'+host+'-epoch4-gates.json','gates-'+host+'.json')
for host in ['macos','linux','windows']:
 root=S if host=='windows'else P
 receipt(root,'alert-native-owning-epoch4.json'if host=='windows'else host+'-alert-epoch4-owning-gates.json','owning-'+host+'.json')
 if host=='macos':before='macos-alert-epoch4-c-pre-cpp-archive-identities.json';after='macos-alert-epoch4-c-final-owning-archive-identities.json'
 elif host=='linux':before='linux-alert-epoch4-pre-cpp-archive-identities.json';after='linux-alert-epoch4-final-owning-archive-identities.json'
 else:before='native-alert-epoch4-pre-cpp-archives.json';after='native-alert-epoch4-final-owning-archive-identities.json'
 copy(root/before,'archives-'+host+'-before.json');copy(root/after,'archives-'+host+'-after.json')
ordinary=receipt(S,'alert-native-ordinary-epoch4-b.json','ordinary.json')
copy(P/'alert-source-epoch4.json','source-freeze.json');copy(P/'alert-owning-source-hashes-epoch4.json','owning-source-hashes.json')
for name in ['macos-alert-epoch4-linkage.json','run-alert-unix-epoch4.py','run-alert-native-epoch4.py','run-alert-native-owning-epoch4.py','run-alert-native-ordinary-epoch4.py','run-alert-native-ordinary-epoch4-b.py','run-alert-owning-epoch4.py','alert-owned-cpp-build-epoch4.py','owning-linux-cpp-build-alert-epoch4.py','collect-alert-archives-epoch4.py','alert-observer-source-epoch4.json','alert-ordinary-owner-epoch4.rs']:
 copy(P/name)
for row in load(P/'alert-owning-source-hashes-epoch4.json')['helpers']:
 if row['path']!='alert-owned-cpp-build-epoch4.py':copy(S/row['path'])
failed=load(S/'alert-native-ordinary-epoch4.json');copy(S/'alert-native-ordinary-epoch4.json')
for row in failed['steps']:copy(S/row['log'])
counts={}
for host in ['macos','linux','windows']:
 full=next(x for x in gates[host]['steps']if x['name']=='full');match=re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored;',full['results'][0]);assert match;counts[host]=dict(zip(['passed','failed','ignored'],map(int,match.groups())))
outcomes=[]
for row in ordinary['outcomes']:
 text=(S/row['log']).read_text();terminal=re.search(r'terminal turn=(\d+) ([^\r\n]+)',text);assert terminal
 outcomes.append({'name':row['name'],'terminal-turn':int(terminal[1]),'terminal':terminal[2],'native':row['native-before'],'alert-calls':re.findall(r'alert-call ([^\r\n]+)',text),'alert-results':re.findall(r'alert-result ([^\r\n]+)',text),'console':re.findall(r'guest-console ([^\r\n]+)',text)})
summary={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-parent':'1b25f3438f63d8cc256ae05ca4270e675a084163','source-epoch':4,'source-files':25066,'source-bytes':1834179374,'library-gates':counts,'ordinary-programs':{'actual-core':ordinary['core-before'],'outcomes':outcomes},'full-process-goal-complete':False,'upstream-commit':'1b25f3438f63d8cc256ae05ca4270e675a084163','feature-code-unchanged':True,'whole-current-root-products-validated':False,'retained-assist-abi':184,'current-root-abi':264,'scope':'Fresh complete gates on the linear integration with concurrent master; 3808 owning inputs and actual artifacts. Epoch3 remains separately recorded historical evidence.','controller-correction':'Ordinary attempt a built successfully, then stopped because proof-only native metadata was deliberately absent from the frozen compilation graph. Attempt b verified the same core, source, observer and built executable, loaded the unchanged pinned native metadata, and resumed without rebuilding.'}
(V/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
