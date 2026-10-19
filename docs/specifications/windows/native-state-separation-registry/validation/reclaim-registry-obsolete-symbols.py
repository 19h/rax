from pathlib import Path
import datetime,hashlib,json,shutil
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record=S/'registry-obsolete-symbols-cleanup.json'
assert not record.exists()
x=json.loads((S/'registry-obsolete-symbols-inventory.json').read_text())
result={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'before':list(shutil.disk_usage(B)),'scope':'Only inventoried disposable PDB debug symbols in this goal task-owned obsolete build targets; current target-wait-owned is excluded. No executable, archive, source, capture, runtime library, or policy mutation.','removed':[]}
for root in x['roots']:
 for row in root['files']:
  p=Path(row['path'])
  assert p.is_relative_to(B) and not p.is_relative_to(B/'target-wait-owned') and p.suffix.lower()=='.pdb'
  assert p.stat().st_size==row['bytes']
  row['sha256']=hashlib.sha256(p.read_bytes()).hexdigest()
  p.unlink()
  result['removed'].append(row)
result['after']=list(shutil.disk_usage(B))
result['free-bytes-restored']=result['after'][2]-result['before'][2]
record.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'removed':len(result['removed']),'free-bytes-restored':result['free-bytes-restored'],'after':result['after']}),flush=True)
