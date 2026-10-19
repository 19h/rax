"""Copy exact current and failed-attempt evidence; no test selection inferred from names."""
from pathlib import Path
import hashlib,json,shutil
K=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('/Users/int/.assist-token-20261019');D=R/'docs/specifications/windows/native-token-security-attributes';V=D/'validation'
V.mkdir(exist_ok=True)
files={}
for base in [K,S]:
 for p in sorted(base.iterdir()):
  if not p.is_file() or p.suffix not in ['.json','.log','.py','.rs','.ps1']:continue
  if not (p.name.startswith(('macos-attributes-','linux-attributes-','native-attributes-','attributes-','run-attributes-','wait-attributes-','collect-attributes-','check-attributes-','stress-attributes-','prepare-attributes-','cleanup-attributes-','build-attributes-','freeze-attributes-','owning-linux-cpp-build-attributes-'))):continue
  if p.name.startswith('attributes-finalization'):
   if p.name!='attributes-finalization.json' or json.loads(p.read_text()).get('state')!='failed':continue
  elif p.name.startswith(('attributes-final-check-','attributes-explicit-owned-','attributes-rax-commit','attributes-root-','attributes-red-','attributes-final-publication')):continue
  if p.name in files and p.read_bytes()!=files[p.name].read_bytes():
   old=files[p.name];q=V/'local-preflight'/old.name;q.parent.mkdir(exist_ok=True);shutil.copy2(old,q)
  files[p.name]=p
for name in ['timer2-native-host-identity.json','capture-timer2-host-identity.py']:
 files[name]=R/'docs/specifications/windows/native-timer2-arm/validation'/name
rows=[]
for name,p in sorted(files.items()):
 raw=p.read_bytes();q=V/name
 if q.exists() and q.read_bytes()!=raw:
  raise AssertionError('Existing saved evidence differs: '+str(q))
 if not q.exists():shutil.copy2(p,q)
 rows.append({'path':q.relative_to(D).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(K/'attributes-collected-evidence.json').write_text(json.dumps({'files':rows},indent=2)+'\n')
print('Copied',len(rows),'exact validation inputs')
