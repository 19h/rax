from pathlib import Path
import json,shutil
Q=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');D=Path('/Users/int/.assist-continue-20261019/docs/specifications/windows/native-continue');V=D/'validation';summary=json.loads((V/'summary.json').read_text());assert summary['library-gates']=={'macos':{'passed':8079,'failed':0,'ignored':2},'linux':{'passed':8073,'failed':0,'ignored':2},'windows':{'passed':7540,'failed':5,'ignored':2}}
plan=json.loads((Q/'continue-plan.json').read_text());assumptions=plan['assumptions']
assumptions[1].update(status='revised',validation='Confirmed after revision: native and HLE share same-height ownership pruning; nine shared/ten native focused tests, complete three-host gates, active-callback cancellation/forced exit and actual ordinary smoke success.')
assumptions.append({'id':'A4','assumption':'The published alert parent and the original frozen parent have an identical compilation graph for this group.','basis':'The parent transition711876bc->5d475727 changes only prior-feature derived proof; no changed path intersects the frozen25069input graph.','dependent-result':'Linear semantic publication on5d475727 without repeating identical compilation inputs.','stress-test':'Preserve all12owned files and rehash every source input, including symlink-text and compiled case data.','falsification-probe':'Any changed parent path intersects the compilation graph, an owned digest differs, or a complete source rehash fails.','status':'confirmed','validation':'continue-parent-publication-integration.json records exact path difference and unchanged inputs. Native source was independently materialized and hash-checked before each gate.'})
(V/'final-assumptions.json').write_text(json.dumps({'scope':'One Windows29683ARM64 kernel/four ABI views; this semantic group, not the complete process objective.','assumptions':assumptions},indent=2)+'\n')
quality={'scope':'NtContinue semantic group; full process objective remains active.','gates':{'QG1':'pass: no normative judgment required','QG2':'pass: A1-A4 carry explicit bases, dependent results, stress tests and falsification probes','QG3':'pass within group: native reference, complete implementation, parity, all selected gates and owning smoke success; publication audited separately','QG4':'pass: BOOLEAN8bits; context912/1232/716bytes; smoke3115529-2943204=172325additional observerturns after NtContinue','QG5':'pass: exact5prior Windows failures retained; prototype/fixture/mount/JSON/symlink verifier corrections preserved and bounded','QG6':'pass: pinned declaration/native source/PE/runtime/kernel metadata and byte-bound source/artifact/gate/ordinary receipts','QG7':'pass: whoami class3, native producer boundary, public Rust enum and whole-current-root scope recorded separately','QG8':'pass:9shared/10native focus, full3hosts/CAPI183/integration544nonWin4Win/all-targets; real native/WoW64 leaves and actual owning execution','QG9':'pass: module registration/source ownership; opaque CABI/defaults/dependencies unchanged'}};(V/'quality-gates.json').write_text(json.dumps(quality,indent=2)+'\n')
findings=[{'impact':'high','item':'Whoami remains at NtQueryVirtualMemory class3; successful smoke alone does not complete general Windows or full three-platform process support.','evidence':'ordinary.json and exact native/guest output comparison','blocks-current-group':False,'blocks-full-process-goal':True},{'impact':'high','item':'Native non-alertable context replacement previously retained an abandoned APC frame at equal stack height.','evidence':'continue-focus-c7pass1fail, then continue-focus-d9pass0fail and full/native validation','blocks-current-group':False,'resolution':'Shared native/HLE resumed-context pruning handles both alertable and non-alertable replacement.'},{'impact':'medium','item':'Public Rust Flow gains ResumeAlertable; opaque CABI1.14/defaults/dependencies remain unchanged.','evidence':'source diff and all-target/CAPI/consumer gates','blocks-current-group':False},{'impact':'high','item':'Retained ABI184 consumers do not establish whole current-root ABI264 GUI/plugin/package behavior.','evidence':'Owning source manifests and exact consumer selections; parent root pipeline71509 is separate evidence','blocks-current-group':False,'blocks-full-process-goal':True}];(V/'bounded-findings.json').write_text(json.dumps(findings,indent=2)+'\n');(V/'change-surfaces.json').write_text(json.dumps(plan['surface-map'],indent=2)+'\n')
for name in['continue-kernel-profile.json']:shutil.copy2(S/name,V/name)
for name in['read-continue-kernel-profile.py','collect-continue-validation.py','finalize-continue-validation.py']:shutil.copy2(Q/name,V/name)
p=D/'README.md';s=p.read_text();start=s.index('Complete Windows, macOS and Linux gates,');s=s[:start]+'''Complete source-bound validation now records:

| Host | Library passed | Failed | Ignored | C API passed | Integration passed | All targets |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| macOS | 8079 | 0 | 2 | 183 | 544 | pass |
| Linux | 8073 | 0 | 2 | 183 | 544 | pass |
| Windows | 7540 | 5 established lowerer failures | 2 | 183 | 4 | pass |

Nine shared and ten native Windows focused tests pass. The Windows selection
executes actual installed native/WoW64 NtContinue leaves and real CPU callbacks.
The compilation graph contains 25,069 files / 1,834,242,339 bytes. The switch from
the original unpublished parent to published alert commit 5d475727 changes only
prior derived proof, outside every compiled input; all 12 owned files and the
entire frozen source graph remain byte-identical.

Retained Assist ABI 184 validation passes seven macOS CTests and five C++
consumers each on Linux and Windows, with 3,811 inputs, actual Cargo archives,
full member walks, stable before/after identities and six macOS linker commands.
Whole current-root ABI 264 GUI/plugin/package behavior remains separate.

Actual owning Windows core: 508,624,474 bytes, SHA-256
`0654968feb60b5a26d6a6c971c33c569a06e5a78674c8ed1daa55435b56b60b4`.
The ordinary ARM64 smoke guest enters NtContinue at observer turn 2,943,204 with
TestAlert=1, valid flags 0x400017, target PC 0x7fff38b24d30 and SP 0x93ffc0. The
same turn restores PC/SP and GPR0=0x140001048, with no queued APC, frame or wait
status. The restored GPR0 remains the supplied entry argument rather than a
fabricated success status. The guest exits with code 0 at turn 3,115,529:
3,115,529 - 2,943,204 = 172,325 additional observer turns. Native and emulated
smoke stdout/stderr are empty and both exit 0.

Whoami remains at unsupported NtQueryVirtualMemory class 3, turn 2,966,197;
its native control exits 0 and prints the retained SYSTEM identity. Source,
core, observer, executable, program and six runtime DLL identities match before
and after both owning runs. Private byte copies preserve the exact owning
archives and observer executable before later builds reuse their output paths.
These results establish one ordinary Windows smoke completion. The complete
process objective remains active.

The raw prototype, compiled baseline, same-height ownership regression and
controller corrections are preserved in `validation/`. An independent native
query confirms Windows version 10.0 build 29683 and native architecture ARM64.
Assumptions A1-A4, change surfaces, bounded findings and Quality Gates accompany
the exact acceptance ledger. Run `python3 -B check_validation.py --host HOST`
with HOST macos, linux or windows. Native materialization permits only the
recorded 374-byte offline Cargo configuration adaptation.
''';p.write_text(s);print('finalized NtContinue group evidence')
