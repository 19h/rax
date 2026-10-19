from pathlib import Path
import json,shutil
N=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=N/'wait-source/docs/specifications/windows/native-single-wait/validation';M=N.parent/'native-mutant-create-20261018'
D.mkdir(exist_ok=True)
summary={'frozen-rust-abi-minor':184,'compared-root-rust-abi-minor':264,'compared-root-head':'8f49854be133693b69f546748fe207b23c6a23cd','selected-cpp-inputs':8,'whole-current-root-product-validated':False,'hosts':{}}
for host in ['macos','linux']:
 rows=json.loads((N/(host+'-wait-epoch4-owning-gates.json')).read_text());assert all(row['exit-code']==0 for row in rows)
 current=json.loads((N/(host+'-wait-epoch4-final-owning-archive-identities.json')).read_text());parent=json.loads((M/(host+'-mutant-final-owning-archive-identities.json')).read_text())
 assert len(current['sources'])==474 and current['core-features']==[]
 changed=all(a['sha256']!=b['sha256']for a,b in zip(current['archives'],parent['archives']));assert changed
 summary['hosts'][host]={'gates':[{'name':row['gate'],'exit-code':row['exit-code'],'log':row['log']}for row in rows],'archives-changed-from-parent':changed}
 for p in N.glob(host+'-wait-epoch4-*'):
  if p.is_file():shutil.copy2(p,D/p.name)
record=json.loads((S/'wait-native-owning-epoch4.json').read_text());assert record['state']=='finished'and record['exit-code']==0
current=json.loads((S/'native-wait-epoch4-final-owning-archive-identities.json').read_text());parent=json.loads((Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')/'native-mutant-final-owning-archive-identities.json').read_text())
changed=all(a['sha256']!=b['sha256']for a,b in zip(current['archives'],parent['archives']));assert changed
summary['hosts']['windows']={'gates':[{'name':row['name'],'exit-code':row['exit-code'],'log':row['log']}for row in record['steps']],'archives-changed-from-parent':changed}
for p in S.glob('*wait*epoch4*'):
 if p.is_file() and p.suffix in ['.json','.jsonl','.log']:
  # Core gates still running; collect them only when final completion is checked.
  if p.name.startswith('wait-native-epoch4-') and p.name not in ['wait-native-epoch4-preparation.json','wait-native-epoch4-freshness.json']:continue
  shutil.copy2(p,D/p.name)
for name in ['wait-owning-source-hashes-epoch4.json','collect-wait-archives-epoch4.py','prepare-wait-owning-epoch4.py','run-wait-owning-epoch4.py','run-wait-native-owning-epoch4.py','wait-owned-cpp-build-epoch4.py','current-cpp-consumers-wait-root.json']:
 shutil.copy2(N/name,D/name)
for root in [N,D]:(root/'consumer-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print('Collected final epoch4 carrier evidence from all three hosts')
