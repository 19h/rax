from pathlib import Path
import hashlib,json
Q=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=Path('/Users/int/.assist-native-alpc-csr-20261021/docs/specifications/windows/native-alpc-csr');N=D/'native';N.mkdir(exist_ok=True)
def identity(raw):return {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
def locate(name):
 candidates=[p/name for p in [Q,S] if (p/name).exists()];assert candidates,name;raw=candidates[0].read_bytes();assert all(p.read_bytes()==raw for p in candidates),name;return raw
def copy(name):
 raw=locate(name);p=N/name
 if p.exists():assert p.read_bytes()==raw,name
 else:p.write_bytes(raw)
 return identity(raw)
accepted=[]
groups=[('native-csr-client-connect-v4','csr',None),('native-csr-client-connect-v5','csr',None),('native-csr-client-connect-v6','mapping',None),('native-csr-client-connect-v7','shared-export',None),('native-alpc-csr-trace-v1','trace',None),('native-alpc-private-echo-v4','echo',1),('native-alpc-private-matrix-v5','faults',40),('native-alpc-private-priority-v6','priority',33),('native-alpc-private-alignment-v7','alignment',11),('native-alpc-private-limits-v9','limits',23),('native-alpc-private-priority-v12','grant-priority',12)]
for prefix,kind,count in groups:
 launch=prefix+'-four-views-launch.json';r=json.loads(locate(launch));assert r['state']=='finished' and r['exit-code']==0,prefix;source=prefix+'.cpp';assert identity(locate(source))=={k:r['source-before'][k] for k in ['bytes','sha256']},prefix
 source_id=copy(source);launch_id=copy(launch)
 driver='run-'+prefix+'-four-views.py'
 if not (Q/driver).exists():driver='run-'+prefix+'.py'
 copy(driver)
 for step in r['steps']:copy(step['log'])
 accepted.append({'prefix':prefix,'kind':kind,'calls-per-view':count,'profiles':[x['profile'] for x in r['outcomes']],'source':source,'source-sha256':source_id['sha256'],'launch':launch,'launch-bytes':launch_id['bytes'],'launch-sha256':launch_id['sha256'],'driver':driver})
rejected=[]
for prefix,reason in [('native-csr-client-connect-v1','Historical PULONG length ABI returned NO_MEMORY; not a current CSR reply oracle.'),('native-csr-client-connect-v2','Explicit directory but historical PULONG ABI still wrong.'),('native-csr-client-connect-v3','Bootstrap/capture allocation controls succeeded; PULONG assumption rejected.'),('native-alpc-private-echo-v1','952B NtCreatePort request rejected before creation.'),('native-alpc-private-echo-v2','Echo succeeded in native64 views; global handle count changed during initial runtime threading.'),('native-alpc-private-echo-v3','WoW ALPC client24B vs legacy server40B conversion and volatile global handle counts require corrected accounting.'),('native-alpc-private-limits-v8','New ALPC plus legacy server setup did not complete connection. Exact owned child stopped. Reason unknown; not an accepted transport oracle.'),('native-alpc-private-priority-v10','ALPC observations completed; auxiliary fixed64B basic query failed. Printed zero granted value is unknown. Replaced by V12 successful SDK-size query.'),('native-alpc-private-limits-v11','Zero-length basic query did not supply nonzero size. No ALPC call observation; replaced by V12 SDK-size query.')]:
 names=[prefix+'.cpp',prefix+'-four-views-launch.json']
 for candidate in ['run-'+prefix+'-four-views.py','run-'+prefix+'.py']:
  if(Q/candidate).exists():names.append(candidate);break
 launch=json.loads(locate(prefix+'-four-views-launch.json'));names.extend(x['log'] for x in launch.get('steps',[]));files=[{'path':name,**copy(name)} for name in names]
 rejected.append({'prefix':prefix,'reason':reason,'files':files,'state':launch['state']})
for name in ['alpc-private-limits-v8-owned-child-stop-v1.json','stop-alpc-private-limits-v8-owned-child-v1.py']:
 if(Q/name).exists() or (S/name).exists():copy(name)
(D/'native-producers.json').write_text(json.dumps({'accepted':accepted,'rejected-controls':rejected,'kernel-scope':'Four execution views on one Windows10.0.29683 ARM64 kernel; no independent physical x64 kernel oracle.'},indent=2)+'\n')
print('accepted',len(accepted),'rejected controls',len(rejected))
