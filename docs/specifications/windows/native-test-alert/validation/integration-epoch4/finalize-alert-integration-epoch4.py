from pathlib import Path
import json
P=Path(__file__).parent;R=Path('/Users/int/.assist-alert3-20261019');D=R/'docs/specifications/windows/native-test-alert';V=D/'validation/integration-epoch4';summary=json.loads((V/'summary.json').read_text());expected={'macos':{'passed':8070,'failed':0,'ignored':2},'linux':{'passed':8064,'failed':0,'ignored':2},'windows':{'passed':7530,'failed':5,'ignored':2}};assert summary['library-gates']==expected
for host in ['macos','linux','windows']:
 for prefix in ['gates','owning']:
  d=json.loads((V/(prefix+'-'+host+'.json')).read_text());assert d['state']=='finished'and d['exit-code']==0
p=V/'assumptions.json';d=json.loads(p.read_text());assert d['assumptions'][0]['id']=='A5';d['assumptions'][0].update(status='confirmed',validation='All nine protected feature paths unchanged; fresh full/CAPI/integration/all-targets selections on all three hosts, 3808 owning inputs, stable actual Cargo archives/consumer outputs and both ordinary runs completed. Windows retains exactly the established five lowerer failures.');p.write_text(json.dumps(d,indent=2)+'\n')
quality={'scope':'Final source integration on1b25f343; full process goal remains active and whole current-root products remain outside these retained consumers.','gates':{'QG1':'pass: no normative judgment required','QG2':'pass: historical A1-A4 retained and integration A5 confirmed with exact source/receipt probes','QG3':'pass within semantic group: fresh source integration and all selected runtime/consumer gates; publication audited separately','QG4':'pass: source25066files1834179374bytes; owning3808inputs; smoke2943204-2943177=27observerturns','QG5':'pass: exact five prior Windows failures retained, harness resume qualified, current and historical epochs separated','QG6':'pass: unchanged native corpus/source/protected paths plus fresh source/artifact/ordinary identities','QG7':'pass: remaining producers, NtContinue and memory-class3/full-product scope remain explicit','QG8':'pass: complete three-host selections, native/WoW64 focus and rebuilt owning closure','QG9':'pass: module registration, public Rust field initialization and unchanged opaque C ABI/defaults/dependencies'}};(V/'quality-gates.json').write_text(json.dumps(quality,indent=2)+'\n')
findings=[{'impact':'high','item':'Smoke still stops at NtContinue alertable APC delivery and whoami at virtual-memory class3; neither guest exits.','evidence':'ordinary.json and source/core/observer/exe/program/runtime before-after identities','blocks-current-group':False,'blocks-full-process-goal':True},{'impact':'medium','item':'Concurrent upstream changed111paths, including shared SMIR/decode/Linux/BPF surfaces.','evidence':'overlay.json and full fresh three-host/owning source integration receipts','blocks-current-group':False,'resolution':'Linear integration preserves all upstream changes; nine feature paths are byte-identical; no merge or force-push.'},{'impact':'high','item':'Retained Assist ABI184 consumers do not prove whole current-root ABI264 GUI/plugin/packages.','evidence':'Owning receipts/source manifests and exact CTest/CPP selections','blocks-current-group':False,'blocks-full-process-goal':True}];(V/'bounded-findings.json').write_text(json.dumps(findings,indent=2)+'\n')
p=D/'README.md';s=p.read_text().replace('## Final validation','## Pre-integration validation (source epoch3)',1);assert '## Current integrated source validation'not in s;s+='''
## Current integrated source validation

Concurrent master advanced to `1b25f3438f63d8cc256ae05ca4270e675a084163`.
The linear integration preserves its 111 changed paths and the byte-identical
nine feature-source paths. `validation/integration-epoch4/overlay.json` records
all before/after identities. Epoch3 above remains historical proof; the current
source was rebuilt and exercised separately:

| Host | Library passed | Failed | Ignored | C API passed | Integration passed | All targets |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| macOS | 8070 | 0 | 2 | 183 | 544 | pass |
| Linux | 8064 | 0 | 2 | 183 | 544 | pass |
| Windows | 7530 | 5 established lowerer failures | 2 | 183 | 4 | pass |

The current compilation graph contains 25066 files and 1834179374 bytes.
Derived feature evidence is excluded except the compiled modeled-case dataset.
The owning closure contains 3808 inputs. Seven macOS CTests, five Linux and five
Windows C++ consumers, full archive member walks and six macOS linker commands
pass; corresponding archive identities remain stable before and after checks.
These remain retained ABI184 consumers, distinct from whole current-root ABI264
GUI/plugin/package validation.

Current actual owning core: 508200048 bytes, SHA-256
`bc6d2a0a6e1f59d75f3db0bad382db820723b162dce8ebb5dcc331e40acbb120`.
Smoke again receives NtTestAlert status0 at turn2943177 and advances27observer
turns to NtContinue at2943204. Its supplied continuation is valid, with flags
0x400017, PC0x7fff38b24d30 and SP0x93ffc0. Whoami remains at memory class3,
turn2966197. Both native controls exit0. Current source, core, observer,
executable, program and six runtime-DLL identities remain stable. Neither
emulated guest exits successfully; the full process objective remains active.

The first current ordinary controller completed its build, then stopped because
proof-only native metadata was absent from the frozen compilation graph. The
corrected controller verifies and reuses that exact built executable/core/source
and reads the unchanged pinned native metadata separately. Both raw receipts are
preserved. Current source checks run in addition to the complete historical
checker; they do not substitute a source overlay for fresh executable evidence.

Current integration assumption A5, Quality Gates and bounded findings are in
`validation/integration-epoch4/`. Publication uses the final proof envelope and
independent portable verification on all three operating systems.
''';p.write_text(s);print('finalized integrated source evidence')
