from pathlib import Path
import datetime,hashlib,json,subprocess,sys
B=Path(__file__).resolve().parent;D=B/'rax-source/docs/specifications/windows/native-wnf-system-subscribe';V=D/'validation'
def main():
 subprocess.run([sys.executable,'-B',str(D/'validation-check.py')],check=True)
 stress=json.loads((B/'system-validation-stress-results.json').read_text())
 if len(stress)!=5 or not all(x['rejected']for x in stress):raise ValueError('incomplete adversarial evidence checks')
 review={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'selected System WNF read subscriptions','complete-user-goal':False,'assumptions':{'S1':'3456 native permission calls and all shared/installed replay checks pass for the bounded profile','S2':'1344 native event cases,3360 modeled four-ABI sequences and2376 installed cases;preserve pending data and event ordering','S3':'shared System non-read notification state remains explicitly unsupported','S4':'zero-length initialized data confirmed by192 fresh native cases'},'library-results':{'macos':[7865,0,2],'linux':[7859,0,2],'windows':[7295,5,2]},'known-native-failures':5,'selected-owning-inputs':332,'quality-gates':{'QG1':'descriptive engineering only','QG2':'assumptions tested, retained or explicitly rejected','QG3':'read-subscription group complete; general System events/full process goal incomplete','QG4':'byte extents, counts, mask equations and turn deltas checked','QG5':'initial assertion-order error and stale wait guard retained; production unchanged across epochs','QG6':'source and native identities bound;exact old native failures preserved','QG7':'remaining native image-extension/unwind boundary explicit'},'ordinary-success':False,'ordinary-turns':[2144480,2146842],'ordinary-delta':2596,'remaining-high-impact':['native executable image extension ownership','secondary SEH frame lookup/handling remains unproven','general shared System notifications and delivery']}
 (B/'system-final-review.json').write_text(json.dumps(review,indent=2)+'\n')
 m=json.loads((V/'results.json').read_text())
 for n in ['stress-system-validation.py','system-validation-stress-results.json','system-final-review.json','finalize-system-review.py']:
  raw=(B/n).read_bytes();p=V/n
  if p.exists()and p.read_bytes()!=raw:raise ValueError('refuse replacing final proof '+n)
  p.write_bytes(raw);m['files']=[x for x in m['files']if x['path']!=n];m['files'].append({'path':n,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
 m['files'].sort(key=lambda x:x['path']);(V/'results.json').write_text(json.dumps(m,indent=2)+'\n')
 p=D/'README.md';t=p.read_text().replace('Native profile confirmed; portable adapter validation pending.','Native profile and portable adapter checks pass.').replace('Native profile confirmed; four-ABI replay pending.','Native profile, four-ABI replay and installed leaves pass.').replace('Cross-platform source and executable validation is pending.','Cross-platform source and executable validation is retained below.').replace('remain retained; validation has not been declared complete.','remain retained; complete validation is recorded below.')
 marker='\n## Completed validation\n'
 if marker not in t:
  t+=marker+'''
| Host | Full library passed/failed/ignored | C API | Scoped embedding |
| --- | --- | --- | --- |
| macOS ARM64 | 7865/0/2 | 168 | 7 CTests, 2 scans, 2 complete archives |
| Linux x86-64 container | 7859/0/2 | 168 | 5 C++ consumers, 2 complete archives |
| Windows ARM64 | 7295/5/2 | 168 | 5 C++ consumers, 2 complete archives |

All libraries ran unfiltered. Windows retains the same four BZHI and one FP16
lowerer failures, checked by exact name. Both ignored microkernel tests remain
explicit. All-target checks pass on each host. WNF focus/regression counts are
7/40 on each Unix host and 8/46 on Windows. Shared Windows integration runs
544 cases; native Windows memory integration runs 4.

Four guest profiles replay 3360 supported sequences and explicitly refuse 2016
unsupported shared-event requests. The installed ARM64, x86 and x86-LAA leaves
replay 2376 cases. Each scoped Assist carrier binds 332 source/fixture inputs,
empty core features and two complete archives. Frozen Assist ABI minor 184 is
distinct from current minor 186. Whole-current-root, GUI, plugin and package
validation are not established. Five altered/incomplete proof sets are rejected.

The final unchanged observer reaches turns 2144480 and 2146842 for smoke and
whoami, respectively: both advance 2596 turns past the previous subscription
boundary. The next raw trace is an unmapped instruction fetch at
`0x7ff82ff430c0`, followed by a cyclic SEH-frame diagnostic at
`0x7ff82b0dc4bc`. Both are unsuccessful application runs; the absent third
fixture is excluded. Native image-extension ownership is the next investigation.
'''
 p.write_text(t)
 p=B/'rax-source/src/user/windows/native-runtime.md';t=p.read_text();marker='\nFinal System-WNF validation binds 332 inputs'
 if marker not in t:t+='''
Final System-WNF validation binds 332 inputs and complete archives on Windows,
macOS and Linux. Full library passed/failed/ignored counts are 7295/5/2,
7865/0/2 and 7859/0/2 respectively; the five Windows lowerer failures are
unchanged. Smoke and whoami advance 2596 turns past the subscription boundary,
then fail an instruction fetch and SEH frame walk. Neither completes normally.
The scoped frozen Assist ABI 184/current186 boundary remains explicit.
'''
 p.write_text(t)
 subprocess.run([sys.executable,'-B',str(D/'validation-check.py')],check=True)
 print('Final System WNF review and documentation bound',flush=True)
if __name__=='__main__':main()
