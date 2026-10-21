from pathlib import Path
import datetime,hashlib,json,shutil,subprocess
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
plan=json.loads((S/'alpc-native-resources-v1.json').read_text());assert not plan['old-cache-processes'].strip()
assert json.loads((S/'alpc-candidate1-windows.json').read_text())['state']=='failed'
root=B/'src-process-mitigation-candidate2/target/normal-candidate2/debug'
receipt=S/'alpc-obsolete-symbols-cleanup-v1.json';assert not receipt.exists()
j={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'checking','free-before':shutil.disk_usage(B).free,'removed':[],'scope':'Only explicit inspected old task-owned PM standalone primary symbols/intermediate objects/import/static libs; preserve every .exe/.dll/.rlib, native producer, source and actual-owning retained4artifact copies. Prior current native attempt terminal link failure; no old-cache cargo/rustc/observer processes.'}
def save():receipt.write_text(json.dumps(j,indent=2)+'\n')
# Verify all four independent actual-owning retained identities before cleanup.
retained=json.loads((S/'process-mitigation-retained-owning-candidate2.json').read_text())
j['retained-receipt-before-sha256']=hashlib.sha256((S/'process-mitigation-retained-owning-candidate2.json').read_bytes()).hexdigest()
paths=list((B/'src-process-mitigation-candidate2/target/retained-owning-candidate2').iterdir())
assert len(paths)==4,paths
j['retained-before']=[{'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in paths]
save()
for row in plan['cleanup-candidates']:
 p=Path(row['path']);assert p.parent in [root,root/'deps'] and p.name.startswith(('rax','librax')) and p.suffix.lower() in ['.pdb','.rmeta','.o','.obj','.lib']
 st=p.stat();assert st.st_size==row['bytes'];r={**row,'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'links':st.st_nlink,'file-id':st.st_ino};p.unlink();assert not p.exists();j['removed'].append(r);save()
j['retained-after']=[{'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in paths];assert j['retained-before']==j['retained-after'];j.update(state='finished',**{'free-after':shutil.disk_usage(B).free,'removed-logical-bytes':sum(x['bytes'] for x in j['removed'])});save();print(json.dumps({'state':j['state'],'free-before':j['free-before'],'free-after':j['free-after'],'removed-count':len(j['removed'])}))
