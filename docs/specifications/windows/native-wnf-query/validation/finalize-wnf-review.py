from pathlib import Path
import datetime, hashlib, json, subprocess, sys
B=Path(__file__).resolve().parent
D=B/'rax-source/docs/specifications/windows/native-wnf-query'
V=D/'validation'
def main():
 subprocess.run([sys.executable,'-B',str(D/'validation-check.py')],check=True)
 rows=json.loads((B/'wnf-validation-stress-results.json').read_text())
 if len(rows)!=5 or not all(x['rejected']for x in rows):raise ValueError('incomplete proof stress cases')
 review={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'NtQueryWnfStateData semantic group','complete-user-goal':False,'assumptions':{'A1':'retained immutable initial-environment snapshot; no live host mutation equivalence','A2':'confirmed guest process state isolation and zero-right current-process handle','A3':'confirmed for1613 metadata values; unknown formats refuse','A4':'confirmed absent host impersonation and owned-thread restoration','A5':'four-ABI portable replay; actual ARM64/x86/x86-LAA leaf execution; x64 V5 identity unverified','A6':'falsified by valid noncurrent session; explicit refusal replaces guessed status'},'bounded-findings':[{'impact':'high','item':'ordinary execution now reaches System WNF subscribe but no application succeeds','blocks-complete-user-goal':True},{'impact':'medium','item':'other Windows builds/physical x64 kernel and live WNF mutation/delivery unmeasured','blocks-this-group':False},{'impact':'medium','item':'Defender removed V5 x64 producer before independent identity check;excluded from verified count','blocks-this-group':False}],'quality-gates':{'QG1':'descriptive engineering only','QG2':'assumptions revisited and false namespace assumption corrected','QG3':'query group implemented and replayed; full process goal explicitly incomplete','QG4':'byte limits, ABI extents, case counts and turn deltas checked','QG5':'known native failures and partial proof scopes explicit','QG6':'native producer hashes, primary declaration and compiled source identities bound','QG7':'remaining boundaries recorded'},'full-libraries':{'macos':[7858,0,2],'linux':[7852,0,2],'windows':[7287,5,2]},'ordinary-success':False,'ordinary-next-service':'NtSubscribeWnfStateChange0x1cd','ordinary-delta-turns':2202,'selected-owning-inputs':327,'proof-stress-cases':5}
 (B/'wnf-final-review.json').write_text(json.dumps(review,indent=2)+'\n')
 m=json.loads((V/'results.json').read_text())
 for name in ['stress-wnf-validation.py','wnf-validation-stress-results.json','wnf-final-review.json','finalize-wnf-review.py','wnf-final-format-check.json']:
  src=B/name;dst=V/name;raw=src.read_bytes()
  if dst.exists()and dst.read_bytes()!=raw:raise ValueError('refuse changing final proof '+name)
  dst.write_bytes(raw);m['files']=[x for x in m['files']if x['path']!=name];m['files'].append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
 m['files'].sort(key=lambda x:x['path']);(V/'results.json').write_text(json.dumps(m,indent=2)+'\n')
 p=D/'README.md';t=p.read_text().replace('Shared replay and installed leaves pass; full suites are pending.','Shared replay, installed leaves and final three-host suites are verified.').replace('Final\nthree-host core and scoped Assist embedding checks are required before this\nchange can be published.','Final\nthree-host core and scoped Assist embedding checks are retained below.')
 marker='\n## Final validation and ordinary continuation\n'
 if marker not in t:
  t+=marker+'''
| Host | Full library passed/failed/ignored | C API | Scoped owning gates |
| --- | --- | --- | --- |
| macOS ARM64 | 7858/0/2 | 168 | 6, including 7 CTests and 2 protected-text scans |
| Linux x86-64 container | 7852/0/2 | 168 | 4, including 5 C++ consumers |
| Windows ARM64 | 7287/5/2 | 168 | Source verification, shipping archive and 5 C++ consumers |

All library runs are unfiltered. The Windows failures are the same four BZHI
and one FP16 lowerer cases checked by exact name in `validation-check.py`.
Both ignored cases are pre-existing microkernel tests. All-target checks pass
on all three hosts; Unix Windows integration runs 544 cases and native memory
integration runs 4. Query metadata/service focus is 4/6 tests on each Unix host
and 6/7 on Windows, including the owned impersonation and installed-leaf cases.

The scoped Assist carriers each bind 327 selected inputs, empty core features
and two structurally complete archives. Their frozen Assist public ABI minor
184 is distinct from current minor 186; RAX ABI remains 1.11.0. These results
do not establish whole-current-root, plugin, GUI or package validation. Five
altered/incomplete proof sets are rejected by the retained stress checker.

The unchanged ordinary observer now stops at `NtSubscribeWnfStateChange`
service `0x1cd`, PC `0x180002d10`: smoke at turn 2141884 and whoami at 2144246.
Each advances 2202 turns from its prior query boundary. The controller exits
successfully after recording an unsupported guest result; neither application
exits successfully. The absent third fixture is explicitly excluded.
'''
 p.write_text(t)
 p=B/'rax-source/src/user/windows/native-runtime.md';t=p.read_text();marker='\nFinal WNF-query validation binds 327 selected inputs'
 if marker not in t:t+='''
Final WNF-query validation binds 327 selected inputs and complete archives on
Windows, macOS and Linux. Full library passed/failed/ignored counts are
7287/5/2, 7858/0/2 and 7852/0/2 respectively; the five Windows lowerer failures
are unchanged. Ordinary smoke/whoami advance 2202 turns to System-scoped
NtSubscribeWnfStateChange 0x1cd and remain incomplete. The scoped frozen Assist
carrier ABI 184/current186 boundary remains explicit in the retained proof.
'''
 p.write_text(t)
 subprocess.run([sys.executable,'-B',str(D/'validation-check.py')],check=True)
 print('Final query review, documentation and stress evidence bound',flush=True)
if __name__=='__main__':main()
