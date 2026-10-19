from pathlib import Path
import json,shutil
Q=Path(__file__).parent
S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
V=Q/'registry-source/docs/specifications/windows/native-state-separation-registry/validation'
V.mkdir(exist_ok=True)
def copy(base,name):
 p=base/name;assert p.is_file(),p
 if base!=Q:shutil.copy2(p,Q/name)
 shutil.copy2(p,V/name)
records=[(Q,'registry-macos-epoch1-gates.json'),(Q,'registry-linux-epoch1-gates.json'),(Q,'macos-registry-epoch1-owning-gates.json'),(Q,'linux-registry-epoch1-owning-gates.json'),(S,'registry-native-epoch1-gates.json'),(S,'registry-native-owning-epoch1.json')]
finished=[]
for base,name in records:
 p=base/name
 if not p.exists():continue
 row=json.loads(p.read_text())
 if row['state']!='finished':continue
 assert row['exit-code']==0
 copy(base,name)
 for step in row['steps']:copy(base,step['log'])
 finished.append(name)
for base,pattern in [(Q,'*-registry-epoch1-*archive*.json'),(S,'native-registry-epoch1-*archive*.json'),(Q,'macos-registry-epoch1-linkage.json')]:
 for p in base.glob(pattern):copy(base,p.name)
for name in ['registry-source-epoch1.json','registry-source-epoch1-archive.json','registry-owning-source-hashes-epoch1.json','registry-macos-prototype.json','registry-macos-prototype-new.log','registry-macos-prototype-existing-ifeo.log','registry-baseline-overrides.json','registry-native-baseline.json','registry-native-baseline.log','registry-native-baseline-b.json','registry-native-baseline-b.log','registry-disk-inspection.json','registry-obsolete-symbols-inventory.json','registry-obsolete-symbols-cleanup.json','registry-owning-unix-source-selection.json','registry-native-negative-controls.json']:
 copy(Q,name)
for p in Q.glob('*.py'):
 if p.name not in ['prepare-registry-plan.py']:
  copy(Q,p.name)
for name in ['redirection-owner-epoch1.rs','redirection-owner-epoch1-build.ps1']:
 copy(Q,name)
for name in ['registry-native-epoch1-preparation.json','registry-native-epoch1-focus.log']:
 if (S/name).exists():copy(S,name)
print('collected terminal groups',len(finished),finished)
