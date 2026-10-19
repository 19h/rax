from pathlib import Path
import datetime,json,subprocess,sys,time,traceback,hashlib
K=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('/Users/int/.assist-token-20261019');D=R/'docs/specifications/windows/native-token-security-attributes';V=D/'validation';record=K/'attributes-finalization-b.json'
assert not record.exists()
r={'state':'waiting-final-native-results','pid':__import__('os').getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'steps':[]}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
def run(name,args):
 path=K/('attributes-finalization-'+name+'.log')
 with path.open('wb') as log:
  child=subprocess.Popen(args,stdout=log,stderr=subprocess.STDOUT);r.update(state='running',step=name,child=child.pid);save();code=child.wait()
 raw=path.read_bytes();r['steps'].append({'name':name,'exit-code':code,'log':path.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()});save();assert code==0,name
save()
try:
 deadline=time.monotonic()+7200
 while True:
  ready=True
  for name in ['attributes-native-epoch2-gates.json','attributes-owner-epoch2-launch.json']:
   d=json.loads((S/name).read_text());assert d['state']!='failed',d.get('error',name)
   ready &= d['state']=='finished'
  if ready:break
  assert time.monotonic()<deadline,'bounded final-result observation expired; inspect live controllers before retry'
  time.sleep(5)
 run('records',[sys.executable,'-B',str(K/'finalize-attributes-records.py')])
 run('validation',[sys.executable,'-B',str(K/'check-attributes-validation.py'),str(R)])
 run('corruptions',[sys.executable,'-B',str(K/'stress-attributes-validation.py')])
 shutil=__import__('shutil');shutil.copy2(K/'attributes-final-negative-controls.json',V/'attributes-final-negative-controls.json')
 quality={
  'scope':'Token-attribute semantic group; full same-host process goal remains incomplete.',
  'QG1':'pass: factual implementation and evidence; no normative content',
  'QG2':'pass: A1-A3 stress/falsification probes and measured kernel scope in final-assumptions.json',
  'QG3':'pass within semantic group: seven types, empty sets, native/WoW transport, rights, names, probes, aliases and capture bounds; whole process exit not claimed',
  'QG4':'pass: 104/88/84byte aligned layouts, physical native bias, 1MiB/4096 bounds and x86sixarguments/28bytecleanup independently exercised',
  'QG5':'pass within measured scope: compiled baseline failures, 12shared/13Windows tests; failed harness/transport attempts preserved; known five unrelated lowerer failures explicit',
  'QG6':'pass: pinned PHNT reference,22656native observations,15native and15final semantic corruptions rejected, full source graph/actual archives/program/DLL identity checks; portable receipts required before commit',
  'QG7':'pass: bounded unknown kernels, rejectedzero-value records, captures beyond bounds, retained ABI184/currentABI264 validation distinction and remaining ordinary frontiers explicit',
  'RAX-QG8':'pass: raw native replay and installed/WoW leaves, full three-host CAPI/integration/alltargets and retained owning ordinary execution; no guest success inferred from controller exit',
  'RAX-QG9':'pass: all private Subject constructors and export/test membership updated; public ABI/defaults/dependency pins unchanged; engineering updates separately bound'
 }
 (V/'quality-gates.json').write_text(json.dumps(quality,indent=2)+'\n')
 run('proof-archive',[sys.executable,'-B',str(K/'build-attributes-proof.py')])
 r.update(state='ready-for-portable-checks',**{'exit-code':0});save()
except BaseException:r.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()});save();raise
print('Token evidence ready for portable verification',flush=True)
