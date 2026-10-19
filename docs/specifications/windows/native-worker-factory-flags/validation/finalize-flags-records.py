from pathlib import Path
import datetime
import hashlib
import json
import re
import shutil
import subprocess
import sys

Q = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R = Q / 'flags-source'
D = R / 'docs/specifications/windows/native-worker-factory-flags'
V = D / 'validation'
assert not (V / 'engineering-source-updates.json').exists(), 'Do not repeat the engineering append.'

owner = json.loads((S / 'flags-owner-epoch1-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
archives = json.loads((S / 'native-flags-epoch1-final-owning-archive-identities.json').read_text())
core = next(row for row in archives['archives'] if row['path'].endswith('.rlib'))
programs = []
for previous, row in zip([2898318, 2900603], owner['outcomes']):
    raw = (S / row['log']).read_bytes()
    match = re.search(r'^terminal turn=(\d+) (.+)$', raw.decode(), re.M)
    assert match and 'NtSetInformationWorkerFactory' in match.group(2) and 'class 2' in match.group(2)
    programs.append({'program': row['program-before'], 'log': row['log'],
                     'log-sha256': hashlib.sha256(raw).hexdigest(),
                     'controller-exit-code': row['exit-code'],
                     'former-stop-turn': previous, 'new-terminal-turn': int(match.group(1)),
                     'additional-observer-turns': int(match.group(1)) - previous,
                     'terminal-result': match.group(2), 'guest-exit-success': False,
                     'native-before': row['native-before'], 'native-after': row['native-after']})
ordinary = {'actual-core': core, 'programs': programs, 'ordinary-guest-exit-success': False,
            'scope': 'Both guests return class13 success and retain flags request1, then reach unsupported class2. Native controls exit0; controller exit0 does not mean guest success.'}
(Q / 'ordinary-program-summary.json').write_text(json.dumps(ordinary, indent=2) + '\n')
shutil.copy2(Q / 'flags-native-negative-controls.json', D / 'flags-native-negative-controls.json')
subprocess.run([sys.executable, '-B', str(Q / 'collect-flags-evidence.py')], check=True)
subprocess.run([sys.executable, '-B', str(D / 'check_native.py'), str(D)], check=True)
subprocess.run([sys.executable, '-B', str(D / 'check_supplement.py'), str(D)], check=True)
subprocess.run([sys.executable, '-B', str(Q / 'check-flags-validation.py'), str(R)], check=True)

plan = json.loads((Q / 'flags-plan.json').read_text())
for row in plan['assumptions']:
    if row['id'] == 'A1':
        row['status'] = 'confirmed within captured dormant contexts; active-worker effects remain unknown'
        row['basis'] += '; native v3 interleaving36controls preserve basic state and v4 second-stack2600controls confirm the same contract at524288/8192bytes; actual owner returnsclass13success with retained1.'
    else:
        row['status'] = 'retained: request bookkeeping only; native default and effective/active flag policy remain unknown'
plan['bounded-scope'][0].update({'finding':'Both ordinary guests returnclass13success and retain1, then advance496turns to unsupported class2; no successful guest exit.', 'evidence':'validation/ordinary-program-summary.json'})
(V / 'final-assumptions.json').write_text(json.dumps({
    'scope': 'Dormant WorkerFactoryFlags requests; full process objective remains active.',
    'assumptions': plan['assumptions'], 'bounded-scope': plan['bounded-scope'],
    'surface-map': plan['surface-map'],
}, indent=2) + '\n')
quality = {
    'scope':'Dormant class13 semantic group; full same-host process objective incomplete.',
    'QG1':'pass: factual implementation and validation; no normative content',
    'QG2':'pass: measured dormant assumption confirmed; native default/effective/active flags remain explicit unknowns',
    'QG3':'pass within group: exact4byte capture/order/grants, alias lifetime, independent request state, one-shot guards/failure atomicity, native2600replays plus36interleaving and2600second-stack controls, six shared/seven Windows tests, three-host gates and retained consumers',
    'QG4':'pass: ULONG32, input4bytes; native/WoW64 signature4args and x86return20bytes; stack1048576/4096 and524288/8192bytes; exact source/case/count/width evidence',
    'QG5':'pass within observed scope: baselineBcompiles0/3fails, baselineAcompiler error excluded; faults/denials retain state, constructors/exhaustive matches updated; class2 and incomplete exit explicit',
    'QG6':'pass: pinned PHNT declaration/license and original native source/PE/module/status/backing bytes, exact source/real archive/program binding and portable verifiers; supplementary evidence separately scoped',
    'QG7':'pass: active effects/default unknown, one kernel, known five lowerer failures, retainedABI184/current-product bound, next class2 frontier and original baselineA error retained',
    'RAX-QG8':'pass: all2600mainrows replay; installed native/WoW64leaves and x86cleanup, complete full selections/actual owning status+retainedrequest, no skipped assertions',
    'RAX-QG9':'pass: shared service/object/constructors/matches/tests/docs align; public Rust enum source shape documented, CAPI1.14/defaults/pins unchanged; runtime append separately bound',
}
(V / 'quality-gates.json').write_text(json.dumps(quality, indent=2) + '\n')
native = json.loads((V / 'flags-native-epoch1-gates.json').read_text())
(V / 'windows-existing-failures.json').write_text(json.dumps({'failures': native['exact-existing-failures']}, indent=2) + '\n')
summary = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'source-epoch': 1, 'source-inputs': 24045, 'source-bytes': 1731467640,
           'core-counts': {'macos': [8031, 0, 2], 'linux': [8025, 0, 2], 'windows': [7492, 5, 2]},
           'shared-focus': 6, 'windows-focus': 7, 'capi-each': 183,
           'native-setter-cases': 2600, 'native-interleaving-controls': 36, 'native-second-stack-controls': 2600, 'native-negative-controls': 12,
           'owning-selected-inputs': 540, 'ordinary-programs': ordinary,
           'full-process-objective-complete': False}
(V / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')

readme = (Q / 'README.pending.md').read_text().replace('ACTUAL_CORE_BYTES',str(core['bytes'])).replace('ACTUAL_CORE_SHA',core['sha256'])
(D / 'README.md').write_text(readme)
shutil.copy2(Q / 'check-flags-validation.py', D / 'check_validation.py')

runtime = R / 'src/user/windows/native-runtime.md'
before = runtime.read_bytes()
manifest = json.loads((Q / 'flags-candidate-epoch1.json').read_text())
row = next(row for row in manifest['files'] if row['path'] == 'src/user/windows/native-runtime.md')
assert len(before) == row['bytes'] and hashlib.sha256(before).hexdigest() == row['sha256']
append = (Q / 'runtime-append.pending.md').read_text().replace('ACTUAL_CORE_BYTES',str(core['bytes'])).replace('ACTUAL_CORE_SHA',core['sha256']).encode()
runtime.write_bytes(before + append)
(V / 'engineering-source-updates.json').write_text(json.dumps({
    'path': row['path'], 'before-sha256': row['sha256'], 'before-bytes': row['bytes'],
    'after-sha256': hashlib.sha256(before + append).hexdigest(), 'after-bytes': len(before + append),
    'scope': 'Append-only engineering record after compiled gates; no Rust/test/build/fixture/ABI change.',
}, indent=2) + '\n')
print('Final WorkerFactoryFlags records written; portable proof and exact publication remain.')
