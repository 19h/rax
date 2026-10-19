from pathlib import Path
import json,shutil
Q=Path(__file__).parent;D=Path('/Users/int/.assist-continue-20261019/docs/specifications/windows/native-continue');V=D/'validation/epoch3';s=json.loads((V/'summary.json').read_text());assert s['library-gates']=={'macos':{'passed':8082,'failed':0,'ignored':2},'linux':{'passed':8076,'failed':0,'ignored':2},'windows':{'passed':7543,'failed':5,'ignored':2}}
a=json.loads((Q/'continue-epoch3-assumptions.json').read_text())
for row in a['assumptions']:row['status']='confirmed';row['validation']='Fresh full/CAPI/integration/all-target gates and owning closure pass on all three hosts;13selected combined wait/continuation prototype checks,12shared/13native focus and native supplemental16/32/28 agree. Exact source/archive/program/runtime identities are stable.'
(V/'assumptions.json').write_text(json.dumps(a,indent=2)+'\n')
quality={'scope':'Current epoch3 candidate, with historical successful and rejected candidates retained separately. Full process goal and whole-current-root validation remain separate.','gates':{'QG1':'pass: no normative judgment required','QG2':'pass: A5 active callback ownership and A6 upstream integration confirmed with falsification probes; historical A1-A4 retained','QG3':'pass within semantic group: current source, native controls, complete three-host gates and both owning execution boundaries; publication audited separately','QG4':'pass: source25071files1834290186bytes;3812owning inputs; BOOLEAN8bits, context912/1232/716bytes; distinct observer/C API slice granularity','QG5':'pass: failed exact-target baseline and rejected descriptor-only wait regression preserved; current ownership preserves both invariants; Windows retains exact5known lowerer failures','QG6':'pass: pinned primary/native source/kernel/runtime/PE/projection and current source/artifact/ordinary/C ABI identities','QG7':'pass: whoami class3, native producer/debugger context boundary, public Rust enum and current-root CI disk limit remain explicit','QG8':'pass: native68resumes136postchecks76callbacks,12shared13native tests, full3hosts/CAPI183/integration544nonWin4Win/all-targets and rebuilt owning consumers','QG9':'pass: module registration/explicit ownership; public CABI/defaults/dependencies unchanged'}};(V/'quality-gates.json').write_text(json.dumps(quality,indent=2)+'\n')
findings=[{'impact':'high','item':'Whoami remains at virtual-memory class3. Smoke succeeds through actual owning core and retained C++/C ABI; this does not complete all Windows or all-platform userland behavior.','evidence':'ordinary.json/capi.json, native controls and before-after source/artifact/runtime identities','blocks-current-group':False,'blocks-full-process-goal':True},{'impact':'high','item':'Epoch2 identified an active callback by descriptor alone, dropping a wait parent using that descriptor in existing preservation coverage.','evidence':'rejected-epoch2-macos/linux/windows receipts, exact-target baseline, current combined selected checks and full suites','blocks-current-group':False,'resolution':'Require a published active callback marker as well as private descriptor identity; preserve existing assertion and checked retry/wait policy.'},{'impact':'medium','item':'C ABI debugger context editing is a separate path from guest NtContinue; no debugger lifecycle policy changes.','evidence':'capi/src/process/runtime.rs WriteContext and guest Flow dispatch paths','blocks-current-group':False},{'impact':'high','item':'Retained Assist ABI184 linked consumers do not establish whole current-root GUI/plugin/package behavior. Publication base root ABI270 includes concurrent mirror/history changes. Parent pipeline71509: five package builds passed; Windows x64 checkout failed authentication; macOS unit compile exhausted runner disk; Linux failed four tests; Windows process-tool test failed NT directory parent identity. Current base pipeline71512 independently repeats Windows namespace-parent and macOS disk failures.','evidence':'owning manifests/selections; pipeline71509/jobs424839,424843,424838,424840 on root1c06f967; pipeline71512/jobs424872,424871 on rootc2eb730d','blocks-current-group':False,'blocks-whole-current-root-proof':True}];(V/'bounded-findings.json').write_text(json.dumps(findings,indent=2)+'\n')
for name in['collect-continue-epoch3.py','finalize-continue-epoch3.py','continue-epoch3-source-review.json','continue-source-rehash-before-publication-epoch3.json']:shutil.copy2(Q/name,V/name)
# Label earlier final-looking tables as historical, then report the current ledger.
p=D/'README.md';text=p.read_text().replace('## Validation status','## Historical epoch1 validation',1);assert '## Current epoch3 validation'not in text;text+='''
## Current epoch3 validation

A later exact-target regression compiled and failed 0 passed / 1 failed: a
callback that restores its exact interrupted PC/SP must abandon its APC owner.
Native supplemental controls complete 16 additional resumes, 32 post-checks and
28 callbacks across the same four ABI profiles. Together with the base corpus,
the native boundary contains 68 resumes, 136 post-checks and 76 callbacks.

Descriptor identity alone proved insufficient in the next broad run: the
existing interrupted-wait test failed. The current rule requires both the
private APC descriptor and an active callback stack marker. It clears abandoned
APC callbacks while retaining a suspended wait parent and the checked retry
frontier. Both native and HLE exact-target cases, plus native/WoW64 CPU callbacks
that invoke the installed NtContinue leaf themselves, are covered. All rejected
attempts remain under validation/epoch3; none is used as current passing proof.

Concurrent upstream `b79c32ca2839568f5c06d123d2f8243e490ba2a4` changes nine
workflow/tooling/reference paths. Runtime source and all twelve owned feature
paths are preserved. Fresh current-source results are:

| Host | Library passed | Failed | Ignored | C API passed | Integration passed | All targets |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| macOS | 8082 | 0 | 2 | 183 | 544 | pass |
| Linux | 8076 | 0 | 2 | 183 | 544 | pass |
| Windows | 7543 | 5 established lowerer failures | 2 | 183 | 4 | pass |

The complete compilation graph has 25,071 files / 1,834,290,186 bytes. The owning
closure has 3,812 inputs. Seven macOS CTests, five C++ consumers on Linux and
Windows, actual archive member walks, stable before/after identities and six
macOS linker commands pass. The combined prototype checks all twelve shared
continuation cases and interrupted wait preservation together; native focus
executes thirteen checks. Both compiled case datasets and actual installed
native/WoW64 leaves remain part of current validation.

Current owning execution again completes ordinary ARM64 smoke with code0 after
NtContinue restores the exact PC/SP/entry registers. Whoami remains at class3.
The retained Assist C++ RaxProcess adapter and C ABI independently return
EXITED(4)/code0 for smoke and FAILED(5)/class3 for whoami. Current source, core,
carrier, observer, program, executable, native controls and six runtime DLL
identities remain stable. Exact current artifacts/outcomes are recorded in
validation/epoch3/summary.json and the raw receipts.

Historical epoch1 source receipts are checked against their explicit before /
after source overlay; they do not substitute for fresh epoch3 compilation or
execution. A5/A6, current Quality Gates and bounded findings accompany this
ledger. Whole current-root products remain separate from retained ABI184
consumers; the current publication base has Rust ABI270 and concurrent unrelated
changes. The full process objective remains active.

The evidence directory pins Git text conversion off so native CRLF logs and
all manifest-bound bytes survive a Windows checkout. Native source trees are
materialized from their exact source manifests before compilation.

The additional ownership pruning takes O(F) time for F abandoned frames and
O(1) auxiliary space. The alert decision is O(1); callback execution and queue
storage retain their existing scheduler bounds. Twelve additional corruption
controls check the current ledger with relevant log/receipt hashes rebound.
''';p.write_text(text);print('current epoch3 finalization prepared from complete receipts')
