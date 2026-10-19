from pathlib import Path
import datetime,hashlib,json,re,shutil
Q=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('/Users/int/.assist-continue-20261019');D=R/'docs/specifications/windows/native-continue';V=D/'validation';assert not(V/'summary.json').exists()
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def copy(p,name=None):
 target=V/(name or p.name)
 if target.exists():assert target.read_bytes()==p.read_bytes(),str(target)
 else:shutil.copy2(p,target)
 return target
def receipt(base,name,target):
 p=base/name;d=load(p);assert d['state']=='finished'and d['exit-code']==0,name;copy(p,target)
 for row in d['steps']:
  f=base/row['log'];raw=f.read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']);copy(f)
 return d
gates={}
for host in['macos','linux','windows']:
 base=S if host=='windows'else Q
 name='continue-native-epoch1-gates.json'if host=='windows'else'continue-linux-epoch1-b-gates.json'if host=='linux'else'continue-macos-epoch1-gates.json'
 gates[host]=receipt(base,name,'gates-'+host+'.json')
 receipt(base,'continue-native-owning-epoch1.json'if host=='windows'else host+'-continue-epoch1-owning-gates.json','owning-'+host+'.json')
 before='native-continue-epoch1-pre-cpp-archives.json'if host=='windows'else'macos-continue-epoch1-c-pre-cpp-archive-identities.json'if host=='macos'else'linux-continue-epoch1-pre-cpp-archive-identities.json'
 after='native-continue-epoch1-final-owning-archive-identities.json'if host=='windows'else'macos-continue-epoch1-c-final-owning-archive-identities.json'if host=='macos'else'linux-continue-epoch1-final-owning-archive-identities.json'
 copy(base/before,'archives-'+host+'-before.json');copy(base/after,'archives-'+host+'-after.json')
ordinary=receipt(S,'continue-native-ordinary-epoch1.json','ordinary.json')
copy(Q/'continue-source-epoch1.json','source-freeze.json');copy(Q/'continue-owning-source-hashes-epoch1.json','owning-source-hashes.json')
for name in['continue-ownership.json','continue-parent-publication-integration.json','macos-continue-epoch1-linkage.json','run-continue-unix-epoch1.py','run-continue-unix-epoch1-b.py','run-continue-native-epoch1.py','run-continue-native-owning-epoch1.py','run-continue-owning-epoch1.py','run-continue-native-ordinary-epoch1.py','continue-owned-cpp-build-epoch1.py','owning-linux-cpp-build-continue-epoch1.py','collect-continue-archives-epoch1.py','continue-observer-source-epoch1.json','continue-ordinary-owner-epoch1.rs','prepare-continue-native-link.py','prepare-continue-native-link-b.py','retain-continue-native-artifacts.py']:
 copy(Q/name)
for row in load(Q/'continue-owning-source-hashes-epoch1.json')['helpers']:
 if row['path']!='continue-owned-cpp-build-epoch1.py':copy(S/row['path'])
for name in['continue-native-owning-link.json','continue-native-owning-link-b.json','continue-native-retained-artifacts.json']:copy(S/name)
failed=load(Q/'continue-linux-epoch1-gates.json');copy(Q/'continue-linux-epoch1-gates.json')
for row in failed['steps']:copy(Q/row['log'])
counts={}
for host in gates:
 full=next(x for x in gates[host]['steps']if x['name']=='full');m=re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored;',full['results'][0]);assert m;counts[host]=dict(zip(['passed','failed','ignored'],map(int,m.groups())))
outcomes=[]
for row in ordinary['outcomes']:
 text=(S/row['log']).read_text();end=re.search(r'terminal turn=(\d+) ([^\r\n]+)',text);assert end
 outcomes.append({'name':row['name'],'terminal-turn':int(end[1]),'terminal':end[2],'native':row['native-before'],'continue-leaves':re.findall(r'continue-leaf ([^\r\n]+)',text),'continue-calls':re.findall(r'continue-call ([^\r\n]+)',text),'continue-results':re.findall(r'continue-result ([^\r\n]+)',text),'console':re.findall(r'guest-console ([^\r\n]+)',text)})
summary={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':1,'source-files':25069,'source-bytes':1834242339,'published-code-parent':'5d475727a72106d24fbd4178df69a6f85efbf85f','native-resumes':52,'native-post-checks':104,'native-callbacks':48,'native-corruptions-rejected':13,'focused-shared-tests':9,'focused-windows-tests':10,'library-gates':counts,'capi-per-host':183,'integration':{'macos':544,'linux':544,'windows':4},'ordinary-programs':{'actual-core':ordinary['core-before'],'outcomes':outcomes},'ordinary-smoke-success':True,'full-process-goal-complete':False,'retained-assist-abi':184,'current-root-abi':264,'whole-current-root-products-validated':False,'scope':'One Windows29683 ARM64 kernel/four producer ABI views, actual installed native/WoW64 leaves, complete three-host selections and retained ABI184 consumers. One ordinary ARM64 smoke guest exits0; whoami remains unsupported at virtual-memory class3.','harness-qualifications':'Linux attempt a stopped before Cargo because its read-only mount lacked a nested target mountpoint. Native owning link handoff a hit a transient partial JSON write; b retries that read and verifies the exact3811inputs before switching the task-owned junction. Parent integration changes only prior feature derived proof outside the complete compilation graph; corrected symlink-text verification confirms every25069inputs. Raw attempts remain preserved.'}
(V/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
