from pathlib import Path
import hashlib,json,shutil
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=B/'rax-source/docs/specifications/windows/native-base-named-objects';V=D/'validation'
def load(path):
 raw=path.read_bytes();return json.loads(raw.decode('utf-16')if raw[:2]in[b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
def copy(path,dest=V):
 dest.mkdir(parents=True,exist_ok=True);shutil.copyfile(path,dest/path.name)
for name in ['base-final-launch.json','base-final-owned-launch.json']:
 row=load(S/name)
 if row['state']!='finished' or row['exit-code']!=0:raise RuntimeError('incomplete native driver '+name)
if not(S/'base-final-post-run-completion.json').exists():raise RuntimeError('native post-run verification missing')
for host,owning in [('macos',7),('linux',4)]:
 for kind,count in [('final',4),('owning',owning)]:
  rows=load(B/(host+'-base-'+kind+'-gates.json'))
  if len(rows)!=count or any(row['exit-code']for row in rows):raise RuntimeError('incomplete Unix gates '+host+'/'+kind)
 for prefix in [host+'-base-final-',host+'-base-owning-']:
  for path in B.glob(prefix+'*'):
   if path.is_file()and path.suffix in ['.log','.json','.jsonl']:copy(path)
for prefix in ['native-base-final-','assist-native-base-final-','base-final-','native-base-owning-','base-directory-focus','base-directory-prepare','base-owner-observer']:
 for path in S.glob(prefix+'*'):
  if path.is_file()and path.suffix in ['.log','.json','.jsonl'] and '-epoch1' not in path.name and '-epoch2-attempt1' not in path.name:copy(path)
for prefix in ['namespace-storage-', 'namespace-recovery-']:
 for path in S.glob(prefix+'*'):
  if path.is_file() and path.suffix in ['.json','.log']:copy(path)
for path in (B/'epoch2-storage-failure').rglob('*'):
 if path.is_file():
  out=V/'development/storage-failure'/path.relative_to(B/'epoch2-storage-failure');out.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(path,out)
for name in ['partial-macos-focus.log','partial-linux-focus.log','partial-linux-focus.json']:
 copy(B/name)
copy(S/'partial-native-focus.log')
copy(S/'partial-native-focus-run.json')
for path in S.glob('namespace-churn-new*'):
 if path.is_file()and path.suffix in ['.log','.json']:copy(path)
for name in ['native-namespace-churn.rs','namespace-churn-new.ps1']:
 copy(B/name)
for name in ['base-final-source.json','base-owning-source-hashes.json','base-owning-unix-link.json','current-cpp-consumers-base-root.json','upstream-cpp-consumer-compatibility.json','real-worktree-port.json','owned-source-plan.json','prototype-epoch3-source.json','base-directories-epoch1-source.json','prototype-macos-focus.log','prototype-macos-focus-result.json','prototype-linux-focus.log','prototype-linux-focus-result.json','native-stress-results.json']:
 copy(B/name)
for name in ['run-base-final.py','run-base-owning-gates.py','collect-base-archives.py','collect-base-final.py','clear-inactive-namespace-cache.py','recover-namespace-check.py','recover-namespace-check.ps1','finish-after-storage-recovery.py','run-revised-churn-when-ready.py','queue-native-base-completion.py','finish-native-base-epoch2.py','partial-native-focus.ps1','prepare-base-final.py','owning-linux-cpp-build.py','macos-base-owning-commands.json','linux-base-owning-commands.json']:
 copy(B/name)
for name in ['base-owner-observer-build.ps1','run-base-owner-observer.py']:
 if(B/name).exists():copy(B/name,V/'owner-diagnostic')
if(S/'base-owner-observer.rs').exists():copy(S/'base-owner-observer.rs',V/'owner-diagnostic')
for path in (B/'windows').iterdir():
 if path.is_file():copy(path,V/'windows')
for directory in ['prototype-attempt1','prototype-attempt2','prototype-epoch2','focus-selection-failure']:
 for path in (B/directory).rglob('*'):
  if path.is_file():
   out=V/'development'/directory/path.relative_to(B/directory);out.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(path,out)
for name in ['prototype-epoch2-review.json','prototype-baseline.json','prototype-vm-memory-inputs.json','prototype-baseline-fixture-links.json']:
 copy(B/name,V/'development')
rows=load(V/'native-base-final-ordinary-outcomes.json');contract=[]
for row in rows:
 if 'log'in row:
  text=(V/row['log']).read_text(encoding='utf-8-sig')
  row['terminal-lines']=[line for line in text.splitlines()if line.startswith('terminal ')or line=='diagnostic turn budget exhausted']
  if len(row['terminal-lines'])!=1:raise RuntimeError('ordinary terminal not unique '+row['log'])
  contract.append({'program-sha256':row['program-sha256'],'terminal':row['terminal-lines'][0]})
(V/'ordinary-analysis.json').write_text(json.dumps(rows,indent=2)+'\n');(V/'ordinary-contract.json').write_text(json.dumps(contract,indent=2)+'\n')
files=[]
for path in sorted(V.rglob('*')):
 if path.is_file()and path.name!='inputs.json':
  raw=path.read_bytes();files.append({'path':str(path.relative_to(D)),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(V/'inputs.json').write_text(json.dumps({'files':files},indent=2)+'\n')
print('Collected',len(files),'final namespace verification records')
