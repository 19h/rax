from pathlib import Path
import datetime,hashlib,json,shutil,subprocess,sys
Q=Path(__file__).parent;R=Q/'registry-source'
D=R/'docs/specifications/windows/native-state-separation-registry';V=D/'validation'
S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
assert not (V/'engineering-source-updates.json').exists(), 'Final records already attempted; inspect rather than repeating append.'
for base,name in [(Q,'registry-macos-epoch1-gates.json'),(Q,'registry-linux-epoch1-gates.json'),(S,'registry-native-epoch1-gates.json'),(Q,'macos-registry-epoch1-owning-gates.json'),(Q,'linux-registry-epoch1-owning-gates.json'),(S,'registry-native-owning-epoch1.json')]:
 x=json.loads((base/name).read_text());assert x['state']=='finished' and x['exit-code']==0,name
subprocess.run([sys.executable,'-B',str(Q/'collect-registry-evidence.py')],check=True)
subprocess.run([sys.executable,'-B',str(Q/'check-native-redirection.py'),str(D)],check=True)
subprocess.run([sys.executable,'-B',str(Q/'check-registry-validation.py'),str(R)],check=True)
plan=json.loads((D/'plan-and-ownership.json').read_text())
for row in plan['assumptions']:
 row['status']='confirmed'
 if row['id']=='A1':row['basis']+='; inherited Microsoft shared-HKLM view contract and native default/32/64 view adapter checks.'
assumptions={'scope':'Scoped registry semantic group; full process goal remains active and incomplete.','assumptions':plan['assumptions'],'bounded-scope':plan['scope-expansion']+[{'impact':'medium','item':'Native complete core retains exactly five pre-existing AArch64 lowerer failures.','blocks-registry-group':False,'blocks-all-tests-passed-claim':True,'evidence':'registry-native-epoch1-gates.json'},{'impact':'high','item':'Retained consumer validation does not establish whole-current-root GUI/plugin/package builds.','blocks-registry-group':False,'blocks-whole-product-build-claim':True,'evidence':'All three owning gate records; retained Assist ABI184 and eight C++ inputs compared to root ABI264.'}]}
(V/'final-assumptions.json').write_text(json.dumps(assumptions,indent=2)+'\n')
quality={'scope':'Native StateSeparation registry semantic group only; full userland process goal is incomplete.','QG1':'pass: no normative judgment required','QG2':'pass: source-bound assumption register and native falsification probes','QG3':'pass within group: present/absent capture, shared model, all guest ABIs, all three hosts and retained consumers; ordinary guest exit remains false','QG4':'pass: UTF-16 bytes, native masks, source bytes, matrix counts and test counts reproducible','QG5':'pass within group: native root errors distinguished, disappearing child aborts, unknown sibling frontier retained, shared budgets and handle lifetime tested','QG6':'pass: stored Microsoft sources, pinned inherited PHNT references, native producer and archive/program/module identities','QG7':'pass: out-of-scope full-runtime, physical-kernel and current-product limitations explicitly recorded'}
(V/'quality-gates.json').write_text(json.dumps(quality,indent=2)+'\n')
native=json.loads((V/'registry-native-epoch1-gates.json').read_text())
(V/'windows-existing-failures.json').write_text(json.dumps({'failures':native['exact-existing-failures']},indent=2)+'\n')
ordinary=json.loads((V/'ordinary-program-summary.json').read_text())
table='''| Host | Complete core: passed / failed / ignored | New focus | C API | Process integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7,976 / 0 / 2 | 6 passed | 183 passed | 544 passed + 0 cfg cases |
| Linux x86-64 container | 7,970 / 0 / 2 | 6 passed | 183 passed | 544 passed + 0 cfg cases |
| Windows ARM64 | 7,419 / 5 / 2 | 9 passed | 183 passed | 0 cfg cases + 4 native passed |

All three all-target build checks passed. The complete core selections are
unfiltered. The exact five retained Windows failure names are recorded in
`validation/windows-existing-failures.json`.'''
paragraph='''The smoke program advances from observer turn 2,178,927 to 2,188,223;
`whoami` advances from 2,181,212 to 2,190,508. Both advance 9,296 normally
executed observer turns, return `0xC0000034` with a zero handle at the prior
registry stop, and reach the separate unsupported `NtSetTimer2` service
(`0x1C0`). Neither guest exits successfully. The controller exits zero to
record the observation. Source, programs, observer executable, core archive and
six installed runtime modules are unchanged across the observations. The actual
selected Windows core `.rlib` is 503,963,010 bytes with SHA-256
`790ba7cd4d87fe35b63fdbb4aff228cb0b3996a3a2583d09537139fc029db05e`.'''
readme=(Q/'README.pending.md').read_text().replace('FINAL_VALIDATION_TABLE',table).replace('ORDINARY_PROGRAM_RESULTS',paragraph)
assert 'FINAL_VALIDATION_TABLE' not in readme and 'ORDINARY_PROGRAM_RESULTS' not in readme
(D/'README.md').write_text(readme)
for source,destination in [('check-native-redirection.py','check_native.py'),('check-registry-validation.py','check_validation.py')]:shutil.copy2(Q/source,D/destination)
runtime=R/'src/user/windows/native-runtime.md';before=runtime.read_bytes()
manifest=json.loads((Q/'registry-source-epoch1.json').read_text())
row=next(row for row in manifest['files'] if row['path']=='src/user/windows/native-runtime.md')
assert hashlib.sha256(before).hexdigest()==row['sha256'] and len(before)==row['bytes']
append='''

## Native StateSeparation registry evidence (2026-10-19)

Native-library selection now snapshots the fixed SYSTEM
`StateSeparation\\RedirectionMap\\Keys` subtree, using known absence only for
fixed-root `ERROR_FILE_NOT_FOUND` and complete bounded descendants when present.
Guest handles retain immutable records; arbitrary guest names do not invoke
host registry operations. IFEO and the new fixed root share the same key-tree
model and aggregate value budgets. Other keys and prefix lookalikes retain the
explicit unsupported frontier.

The native producer records 256 fixed-key/view observations across ARM64,
x64 compatibility, x86, and x86 LAA on one Windows ARM64 kernel, with stable
producer/source/module identities and handle counts. Of these, 224 lie inside
the selected root and 32 are outside-prefix controls. Twelve native-evidence
corruptions are rejected. The unchanged implementation fails the new native
presence regression (0 passed, 1 failed); the first baseline compile failed
because the guest disk filled, and the distinct retry reached the assertion.
Task-owned obsolete PDB cleanup restored 8,115,089,408 bytes without removing
sources, executables, archives, runtime libraries, or captures.

Six new shared tests pass on macOS/Linux and nine pass on native Windows.
Complete unfiltered core results are macOS 7,976/0/2, Linux 7,970/0/2, and
Windows 7,419/5/2 (passed/failed/ignored), retaining exactly the five established
Windows lowerer failures. All three C API selections pass 183 tests, and all
three all-target checks pass. The source snapshot binds 22,779 inputs and
1,583,474,670 bytes. Actual retained Assist core/carrier archives, 513 selected
inputs, full archive walks before/after C++ links, seven macOS CTests and five
Linux/Windows C++ consumers pass. Retained Assist Rust ABI184 and C++ inputs
compared to root ABI264 remain explicit limitations; this is not a
whole-current-root GUI/plugin/package result. The public RAX C API stays 1.14.0.

Both ordinary Windows programs return the native missing-key status at the
former registry stop and advance 9,296 observer turns. They stop separately at
unimplemented `NtSetTimer2` (0x1C0), without a successful guest exit. The current
Windows owning core is 503,963,010 bytes, SHA-256
`790ba7cd4d87fe35b63fdbb4aff228cb0b3996a3a2583d09537139fc029db05e`.
Sources, commands, logs, exact failure names, native captures and portable
verification are under
`docs/specifications/windows/native-state-separation-registry/`.
'''.encode()
runtime.write_bytes(before+append)
update={'path':row['path'],'before-sha256':row['sha256'],'before-bytes':row['bytes'],'after-sha256':hashlib.sha256(before+append).hexdigest(),'after-bytes':len(before+append),'scope':'Append-only engineering record after all compiled-source gates and retained consumer checks; no Rust, test, manifest, lockfile, fixture or ABI change.'}
(V/'engineering-source-updates.json').write_text(json.dumps(update,indent=2)+'\n')
summary={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':1,'scope':'Scoped registry feature complete, full process goal incomplete','core-counts':{'macos':[7976,0,2],'linux':[7970,0,2],'windows':[7419,5,2]},'capi-tests-each-host':183,'native-cases':256,'native-selected-root-cases':224,'new-shared-tests':6,'new-windows-tests':9,'native-negative-controls':12,'owning-selected-inputs':513,'ordinary-programs':ordinary}
(V/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print('final engineering records written; next: adversarial final checker, three-host proof verification, exact owned red publication')
