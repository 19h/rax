"""Collect only completed final gates and bind the publication engineering record."""
from pathlib import Path
import datetime,hashlib,json,shutil,subprocess
N=Path(__file__).resolve().parent;R=N/'wait-source';D=R/'docs/specifications/windows/native-single-wait';V=D/'validation';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')

def load(path):return json.loads(path.read_text())
for path in [N/'wait-macos-epoch4-gates.json',N/'wait-linux-epoch4-gates.json',S/'wait-native-epoch4-gates.json',S/'wait-native-owning-epoch4.json']:
 data=load(path);assert data['state']=='finished'and data['exit-code']==0,path
ordinary=load(S/'wait-owner-epoch4-launch.json');assert ordinary['state']=='finished'and ordinary['exit_code']==0
V.mkdir(exist_ok=True)
# Source manifests and raw prior attempts remain separate, including rejected
# cache selections, two corrected baseline fixtures and the ordinal failure.
for p in N.iterdir():
 if not p.is_file() or p.suffix not in ['.json','.log','.py','.ps1','.rs','.jsonl']:continue
 if p.name in ['continuation-state.json','wait-root-baseline.json']:continue
 shutil.copy2(p,V/p.name)
for p in S.iterdir():
 if not p.is_file() or p.suffix not in ['.json','.log','.jsonl']:continue
 if p.name.startswith(('wait-native-epoch3','wait-native-epoch4','wait-native-owning-epoch4','native-wait-epoch4','wait-owner-epoch4','wait-final-owned','assist-native-wait-final','native-wait-final','native-wait-owning')):
  shutil.copy2(p,V/p.name)
for host in ['macos','linux']:
 for p in N.glob(host+'-wait-*'):
  if p.is_file():shutil.copy2(p,V/p.name)
for name in ['owning-windows-wait','owning-windows-wait-b']:
 shutil.copytree(N/name,V/name,dirs_exist_ok=True)
subprocess.run(['python3',str(N/'collect-wait-consumers-epoch4.py')],check=True)
# The only tracked source changed after the compiled snapshot is this explicitly
# noncompiled engineering document. Tests, Rust and their TSV data stay frozen.
path=R/'src/user/windows/native-runtime.md';text=path.read_text()
heading='## Native single-object waits and preserved synthetic ordinals (2026-10-19)'
assert heading not in text,'final engineering record already appended'
observed=load(N/'ordinary-program-summary.json')
paragraph='''

'''+heading+'''

NtWaitForSingleObject now uses private guest dispatcher objects, native saved
contexts and cooperative parking. Timeout reads precede handle/access checks;
queued ordinary APCs precede signal consumption. Relative 100 ns intervals retain
an awake-time target, absolute intervals retain NT UTC, and finite extremes
remain finite. Measured low-byte alertable behavior applies to the three
Win32 alertable adapters; WaitAll keeps full BOOL semantics.

The new native export table follows all existing synthetic NTDLL families.
Every previous ordinal and symbol/trap RVA is retained on all three guest ABIs.
The primary native export body is byte-identical to the published mutant parent.

| Host | Full core passed/failed/ignored | Focus | C API |
|---|---|---|---|
| macOS ARM64 | 7961/0/2 | 16 | 168 |
| Linux x86-64 container | 7955/0/2 | 16 | 168 |
| Windows ARM64 | 7401/5/2 | 17 | 168 |

Windows retains exactly the five established lowerer failures. Both ignored
checks remain explicit. All-target checks pass; Unix runs544 integration cases
and Windows runs4 native-memory cases. Native proof retains20 producer runs
on one ARM64 kernel and four ABI profiles, including exact2240 fault/guard and
336 object/APC replay rows, provider discrepancy and13 rejection controls.
The complete final source snapshot binds22059 inputs and1510895073 bytes;
three separate overrides preserve old synthetic ordinals. Retained consumers
bind474 selected inputs and two actual complete Cargo archives per host. The
final Windows helper verifies its actual carrier before and after five C++
consumers. Frozen Assist RustABI184 is distinct from compared root8f49854b /
ABI264; eight selected C++ sources match. Whole-current-root GUI/plugin/package
validation is outside these retained-carrier results.

Both unchanged ordinary programs now return0 from native NtWaitForSingleObject,
with private mutant ownership transferred to the guest thread, and advance351
observer turns to unsupported NtOpenSemaphore(0x136). Neither reaches a
successful guest exit. Broader NT synchronization, private BNO/security contexts
and complete ordinary process support remain required by the full goal.
Public Rust Wait/NativeDeadline source types are extended; public C ABI,
dependencies, defaults and package layout remain unchanged.

[Contract, assumptions and evidence](../../../docs/specifications/windows/native-single-wait/README.md).
'''
path.write_text(text+paragraph)
manifest=load(N/'wait-source-epoch4.json');old=next(row for row in manifest['files']if row['path']=='src/user/windows/native-runtime.md');raw=path.read_bytes()
updates={'files':[{'path':old['path'],'compiled-snapshot-sha256':old['sha256'],'final-sha256':hashlib.sha256(raw).hexdigest(),'final-bytes':len(raw),'scope':'Noncompiled engineering append after all final gates; Rust/test inputs stay identical.'}]}
(N/'engineering-source-updates.json').write_text(json.dumps(updates,indent=2)+'\n');(V/'engineering-source-updates.json').write_text(json.dumps(updates,indent=2)+'\n')
# Bind every evidence byte, preserving raw native line endings and encodings.
rows=[]
for p in sorted(D.rglob('*')):
 if not p.is_file() or p.name=='evidence.json' or '__pycache__'in p.parts:continue
 raw=p.read_bytes();rows.append({'path':p.relative_to(D).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(D/'evidence.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':4,'files':rows},indent=2)+'\n')
print('Final evidence files',len(rows))
