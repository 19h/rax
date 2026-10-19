from pathlib import Path
import datetime,hashlib,json,re,shutil,time
Q=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=Path('/Users/int/.assist-continue-20261019/docs/specifications/windows/native-continue');V=D/'validation/epoch3';V.mkdir(exist_ok=True);assert not(V/'summary.json').exists()
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def copy(p,name=None):
 out=V/(name or p.name)
 if out.exists():assert out.read_bytes()==p.read_bytes(),str(out)
 else:shutil.copy2(p,out)
 return out
def receipt(base,name,target,success=True):
 d=load(base/name);assert d['state']=='finished'and d['exit-code']==0 if success else d['state']=='failed';copy(base/name,target)
 for row in d['steps']:
  f=base/row['log'];raw=f.read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']);copy(f)
 return d
while True:
 try:retained=load(S/'continue-native-retained-artifacts-epoch3.json')
 except(FileNotFoundError,json.JSONDecodeError):retained={}
 if retained.get('state')in['finished','failed']:break
 time.sleep(5)
assert retained['state']=='finished'and retained['exit-code']==0
copy(S/'continue-native-retained-artifacts-epoch3.json');copy(Q/'retain-continue-native-artifacts-epoch3.py')
gates={}
for host in['macos','linux','windows']:
 base=S if host=='windows'else Q
 gates[host]=receipt(base,'continue-native-epoch3-gates.json'if host=='windows'else'continue-'+host+'-epoch3-gates.json','gates-'+host+'.json')
 receipt(base,'continue-native-owning-epoch3.json'if host=='windows'else host+'-continue-epoch3-owning-gates.json','owning-'+host+'.json')
 before='native-continue-epoch3-pre-cpp-archives.json'if host=='windows'else'macos-continue-epoch3-c-pre-cpp-archive-identities.json'if host=='macos'else'linux-continue-epoch3-pre-cpp-archive-identities.json';after='native-continue-epoch3-final-owning-archive-identities.json'if host=='windows'else'macos-continue-epoch3-c-final-owning-archive-identities.json'if host=='macos'else'linux-continue-epoch3-final-owning-archive-identities.json';copy(base/before,'archives-'+host+'-before.json');copy(base/after,'archives-'+host+'-after.json')
 receipt(base,'continue-native-epoch2-gates.json'if host=='windows'else'continue-'+host+'-epoch2-gates.json','rejected-epoch2-'+host+'.json',False)
ordinary=receipt(S,'continue-native-ordinary-epoch3.json','ordinary.json');capi=receipt(S,'continue-owning-capi-epoch3.json','capi.json')
for epoch in [2,3]:copy(Q/('continue-source-epoch'+str(epoch)+'.json'),'source-freeze-epoch'+str(epoch)+'.json')
copy(Q/'continue-source-epoch3.json','source-freeze.json');copy(Q/'continue-owning-source-hashes-epoch3.json','owning-source-hashes.json');copy(Q/'continue-epoch3-overlay.json','overlay.json')
for name in['continue-upstream-integration-epoch3.json','continue-exact-target-baseline.json','continue-exact-target-baseline.log','continue-exact-target-fix.json','continue-exact-target-fix.log','continue-epoch2-focus.json','continue-epoch2-focus.log','continue-epoch3-focus.json','continue-epoch3-focus.log','continue-supplement-negative-controls.json','stress-continue-supplement.py','macos-continue-epoch3-linkage.json','run-continue-unix-epoch3.py','run-continue-native-epoch3.py','run-continue-native-owning-epoch3.py','run-continue-owning-epoch3.py','run-continue-native-ordinary-epoch3.py','run-continue-owning-capi-epoch3.py','continue-owned-cpp-build-epoch3.py','owning-linux-cpp-build-continue-epoch3.py','collect-continue-archives-epoch3.py','continue-observer-source-epoch3.json','continue-ordinary-owner-epoch3.rs']:
 copy(Q/name)
for row in load(Q/'continue-owning-source-hashes-epoch3.json')['helpers']:
 if row['path']!='continue-owned-cpp-build-epoch3.py':copy(S/row['path'])
counts={}
for host in gates:
 m=re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored;',next(x for x in gates[host]['steps']if x['name']=='full')['results'][0]);assert m;counts[host]=dict(zip(['passed','failed','ignored'],map(int,m.groups())))
outcomes=[]
for row in ordinary['outcomes']:
 text=(S/row['log']).read_text();end=re.search(r'terminal turn=(\d+) ([^\r\n]+)',text);assert end
 outcomes.append({'name':row['name'],'terminal-turn':int(end[1]),'terminal':end[2],'native':row['native-before'],'continue-leaves':re.findall(r'continue-leaf ([^\r\n]+)',text),'continue-calls':re.findall(r'continue-call ([^\r\n]+)',text),'continue-results':re.findall(r'continue-result ([^\r\n]+)',text),'console':re.findall(r'guest-console ([^\r\n]+)',text)})
summary={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':3,'source-files':25071,'source-bytes':1834290186,'published-code-parent':'b79c32ca2839568f5c06d123d2f8243e490ba2a4','library-gates':counts,'ordinary-programs':{'actual-core':ordinary['core-before'],'outcomes':outcomes},'ordinary-smoke-success':True,'capi-programs':capi['outcomes'],'native-resumes-total':68,'native-post-checks-total':136,'native-callbacks-total':76,'focused-shared-tests':12,'focused-windows-tests':13,'owning-source-inputs':3812,'full-process-goal-complete':False,'whole-current-root-products-validated':False,'retained-assist-abi':184,'publication-base-root-abi':270,'scope':'Exact target APC abandonment requires both private descriptor identity and active callback marker; interrupted waits preserve ownership. Epoch1 successful ordinary/core evidence remains historical; epoch2 failed broad wait-preservation checks are retained. Fresh epoch3 validates the combined current source and both owning execution boundaries.'};(V/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
