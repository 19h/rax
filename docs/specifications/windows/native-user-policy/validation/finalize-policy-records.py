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
R = Q / 'policy-source'
D = R / 'docs/specifications/windows/native-user-policy'
V = D / 'validation'
assert not (V / 'engineering-source-updates.json').exists(), 'Do not repeat the engineering append.'

owner = json.loads((S / 'policy-owner-epoch1-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
archives = json.loads((S / 'native-policy-epoch1-final-owning-archive-identities.json').read_text())
core = next(row for row in archives['archives'] if row['path'].endswith('.rlib'))
programs = []
for previous, row in zip([2886717, 2889002], owner['outcomes']):
    raw = (S / row['log']).read_bytes()
    match = re.search(r'^terminal turn=(\d+) (.+)$', raw.decode(), re.M)
    assert match and 'NtOpenKey: registry key outside selected runtime snapshot' in match.group(2)
    programs.append({'program': row['program-before'], 'log': row['log'],
                     'log-sha256': hashlib.sha256(raw).hexdigest(),
                     'controller-exit-code': row['exit-code'],
                     'former-stop-turn': previous, 'new-terminal-turn': int(match.group(1)),
                     'additional-observer-turns': int(match.group(1)) - previous,
                     'terminal-result': match.group(2), 'guest-exit-success': False,
                     'native-before': row['native-before'], 'native-after': row['native-after']})
ordinary = {'actual-core': core, 'programs': programs, 'ordinary-guest-exit-success': False,
            'scope': 'Both guests pass the selected per-user policy root and reach the still-unselected FileSystem registry root. Native controls exit0; controller exit0 does not mean guest success.'}
(Q / 'ordinary-program-summary.json').write_text(json.dumps(ordinary, indent=2) + '\n')
shutil.copy2(Q / 'policy-native-negative-controls.json', D / 'policy-native-negative-controls.json')
subprocess.run([sys.executable, '-B', str(Q / 'collect-policy-evidence.py')], check=True)
subprocess.run([sys.executable, '-B', str(D / 'check_native.py'), str(D)], check=True)
subprocess.run([sys.executable, '-B', str(Q / 'check-policy-validation.py'), str(R)], check=True)

plan = json.loads((Q / 'policy-plan.json').read_text())
for row in plan['assumptions']:
    if row['id'] == 'A1':
        row['status'] = 'confirmed within recorded native subject and SID-format corpus'
        row['basis'] += '; native runtime views match the captured primary SID, 512 independent SID-format observations and invalid/other-SID model tests pass.'
    elif row['id'] == 'A2':
        row['status'] = 'confirmed for documented shared path and recorded native views; one physical kernel'
        row['basis'] += '; stored Microsoft shared-key table and HKCU/HKU relation confirm inherited sharing; native 0/32/64 view controls agree.'
    else:
        row['status'] = 'retained: native positive HKU fixture has one key; generic native traversal and synthetic user-policy descendant tests cover the reused tree mechanics'
plan['bounded-scope'][0].update({'finding':'Both ordinary guests pass the per-user policy root and advance2873turns to the still-unselected FileSystem root, without successful guest exit.', 'evidence':'validation/ordinary-program-summary.json'})
(V / 'final-assumptions.json').write_text(json.dumps({
    'scope': 'Fixed primary-user policy root; full process objective remains active.',
    'assumptions': plan['assumptions'], 'bounded-scope': plan['bounded-scope'],
    'surface-map': plan['surface-map'],
}, indent=2) + '\n')
quality = {
    'scope':'Fixed primary-user policy semantic group; full same-host process objective remains incomplete.',
    'QG1':'pass: factual implementation and validation; no normative judgment',
    'QG2':'pass: assumptions, dependencies, stress tests and falsification probes reconciled; native positive-fixture/kernel bounds retained',
    'QG3':'pass within group: primary-SID binding, exact fixed suffix, complete or absent acquisition, shared views, read-only grants, raw values/lifetime/budgets, six shared/nine Windows tests, three-host gates and selected retained consumers',
    'QG4':'pass: SID8+4*N,N<=15; 48bit BE authority and32bit LE subauthorities; current81/max256 UTF16units=162/512bytes; recorded exact source/case/test counts',
    'QG5':'pass within demonstrated scope: root-only ERROR_FILE_NOT_FOUND, other errors fail, SID/suffix/rights boundaries retained; next FileSystem frontier and absence of guest exit explicit',
    'QG6':'pass: stored Microsoft primary sources, independent native producers/PE/modules, twelve rebound native controls, exact source/archive/program bindings and portable verification',
    'QG7':'pass: one physical native kernel, five known lowerer failures, incomplete full objective, selected retained ABI184/current-product bounds and future FileSystem root recorded',
    'RAX-QG8':'pass within group: real tests-only native regression failed before implementation, exact shared/native focus and unfiltered selections checked; candidate and actual owner separated',
    'RAX-QG9':'pass: private source/module registration, feature/default/dependency/CABI consistency; engineering append is separate from compiled source',
}
(V / 'quality-gates.json').write_text(json.dumps(quality, indent=2) + '\n')
native = json.loads((V / 'policy-native-epoch1-gates.json').read_text())
(V / 'windows-existing-failures.json').write_text(json.dumps({'failures': native['exact-existing-failures']}, indent=2) + '\n')
summary = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'source-epoch': 1, 'source-inputs': 23683, 'source-bytes': 1706294072,
           'core-counts': {'macos': [8019, 0, 2], 'linux': [8013, 0, 2], 'windows': [7476, 5, 2]},
           'shared-focus': 6, 'windows-focus': 9, 'capi-each': 183,
           'native-key-opens': 640, 'native-SID-format-observations': 512, 'native-negative-controls': 12,
           'owning-selected-inputs': 535, 'ordinary-programs': ordinary,
           'full-process-objective-complete': False}
(V / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')

readme = (Q / 'README.pending.md').read_text().replace('ACTUAL_CORE_BYTES',str(core['bytes'])).replace('ACTUAL_CORE_SHA',core['sha256'])
(D / 'README.md').write_text(readme)
shutil.copy2(Q / 'check-policy-validation.py', D / 'check_validation.py')

runtime = R / 'src/user/windows/native-runtime.md'
before = runtime.read_bytes()
manifest = json.loads((Q / 'policy-candidate-epoch1.json').read_text())
row = next(row for row in manifest['files'] if row['path'] == 'src/user/windows/native-runtime.md')
assert len(before) == row['bytes'] and hashlib.sha256(before).hexdigest() == row['sha256']
append = (Q / 'runtime-append.pending.md').read_text().replace('ACTUAL_CORE_BYTES',str(core['bytes'])).replace('ACTUAL_CORE_SHA',core['sha256']).encode()
runtime.write_bytes(before + append)
(V / 'engineering-source-updates.json').write_text(json.dumps({
    'path': row['path'], 'before-sha256': row['sha256'], 'before-bytes': row['bytes'],
    'after-sha256': hashlib.sha256(before + append).hexdigest(), 'after-bytes': len(before + append),
    'scope': 'Append-only engineering record after compiled gates; no Rust/test/build/fixture/ABI change.',
}, indent=2) + '\n')
print('Final policy records written; portable proof and exact publication remain.')
