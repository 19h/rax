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
R = Q / 'filesystem-source'
D = R / 'docs/specifications/windows/native-filesystem-registry'
V = D / 'validation'
assert not (V / 'engineering-source-updates.json').exists(), 'Do not repeat the engineering append.'

owner = json.loads((S / 'filesystem-owner-epoch1-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
archives = json.loads((S / 'native-filesystem-epoch1-final-owning-archive-identities.json').read_text())
core = next(row for row in archives['archives'] if row['path'].endswith('.rlib'))
programs = []
for previous, row in zip([2889590, 2891875], owner['outcomes']):
    raw = (S / row['log']).read_bytes()
    match = re.search(r'^terminal turn=(\d+) (.+)$', raw.decode(), re.M)
    assert match and 'NtSetInformationWorkerFactory' in match.group(2) and 'class 13' in match.group(2)
    programs.append({'program': row['program-before'], 'log': row['log'],
                     'log-sha256': hashlib.sha256(raw).hexdigest(),
                     'controller-exit-code': row['exit-code'],
                     'former-stop-turn': previous, 'new-terminal-turn': int(match.group(1)),
                     'additional-observer-turns': int(match.group(1)) - previous,
                     'terminal-result': match.group(2), 'guest-exit-success': False,
                     'native-before': row['native-before'], 'native-after': row['native-after']})
ordinary = {'actual-core': core, 'programs': programs, 'ordinary-guest-exit-success': False,
            'scope': 'Both guests open FileSystem successfully and reach unsupported NtSetInformationWorkerFactory class13. Native controls exit0; controller exit0 does not mean guest success.'}
(Q / 'ordinary-program-summary.json').write_text(json.dumps(ordinary, indent=2) + '\n')
shutil.copy2(Q / 'filesystem-native-negative-controls.json', D / 'filesystem-native-negative-controls.json')
subprocess.run([sys.executable, '-B', str(Q / 'collect-filesystem-evidence.py')], check=True)
subprocess.run([sys.executable, '-B', str(D / 'check_native.py'), str(D)], check=True)
subprocess.run([sys.executable, '-B', str(Q / 'check-filesystem-validation.py'), str(R)], check=True)

plan = json.loads((Q / 'filesystem-plan.json').read_text())
for row in plan['assumptions']:
    if row['id'] == 'A1':
        row['status'] = 'confirmed for documented shared root and recorded three native views; one physical kernel'
        row['basis'] += '; real native snapshot values/presence compare independently in each view.'
    else:
        row['status'] = 'confirmed within complete fixed-tree contract and recorded native/synthetic bounds'
        row['basis'] += '; actual capture, controlled recursive IFEO traversal, child disappearance, error priority, all guest ABIs and the seven-root aggregate budget pass.'
plan['bounded-scope'][0].update({'finding':'Both ordinary guests pass FileSystem and advance8728turns to unsupported NtSetInformationWorkerFactory class13; no successful guest exit.', 'evidence':'validation/ordinary-program-summary.json'})
plan['bounded-scope'].append({'impact':'low','finding':'Executed Unix driver retained the previous artifact prefix; original records/logs and explicit aliases preserve provenance.', 'blocks-group':False, 'evidence':'validation/filesystem-unix-record-promotion.json', 'action':'Verifier proves exact original bytes and metadata with only basename aliases changed.'})
(V / 'final-assumptions.json').write_text(json.dumps({
    'scope': 'Fixed FileSystem registry root; full process objective remains active.',
    'assumptions': plan['assumptions'], 'bounded-scope': plan['bounded-scope'],
    'surface-map': plan['surface-map'],
}, indent=2) + '\n')
quality = {
    'scope':'Fixed FileSystem semantic group; full same-host process objective incomplete.',
    'QG1':'pass: factual implementation and validation; no normative judgment',
    'QG2':'pass: assumptions, dependent results, native/shared tests and falsification probes reconciled',
    'QG3':'pass within group: fixed-root capture or known absence, complete traversal/error handling, shared views, raw values, seven-root budget, read-only grants/lifetime, six shared/nine Windows tests and three-host/retained-consumer gates',
    'QG4':'pass: root61UTF16units=122bytes, observed trailing form62units=124bytes; exact native/source/test counts and existing bounds retained',
    'QG5':'pass within scope: actual native regression, root/error/duplicate/sibling/copy/lifetime/aggregate cases; class13 and absence of guest exit explicit',
    'QG6':'pass: stored Microsoft primary source, exact producer/PE/module/control bindings, source/archive/program identity, portable verifiers and traced original Unix artifacts',
    'QG7':'pass: one physical kernel, five known lowerer failures, retained ABI184/current-product bound, next worker-factory frontier and artifact-prefix issue documented',
    'RAX-QG8':'pass: genuine native tests-only failure before selector edit, complete selections and actual retained-owner ordinary behavior; no test skips weakened',
    'RAX-QG9':'pass: constant/enum/snapshot/test registration agree, no public ABI/default/dependency change, engineering append bound after compiled gates',
}
(V / 'quality-gates.json').write_text(json.dumps(quality, indent=2) + '\n')
native = json.loads((V / 'filesystem-native-epoch1-gates.json').read_text())
(V / 'windows-existing-failures.json').write_text(json.dumps({'failures': native['exact-existing-failures']}, indent=2) + '\n')
summary = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'source-epoch': 1, 'source-inputs': 23843, 'source-bytes': 1716662603,
           'core-counts': {'macos': [8025, 0, 2], 'linux': [8019, 0, 2], 'windows': [7485, 5, 2]},
           'shared-focus': 6, 'windows-focus': 9, 'capi-each': 183,
           'native-key-opens': 320, 'native-negative-controls': 12,
           'owning-selected-inputs': 538, 'ordinary-programs': ordinary,
           'full-process-objective-complete': False}
(V / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')

readme = (Q / 'README.pending.md').read_text().replace('ACTUAL_CORE_BYTES',str(core['bytes'])).replace('ACTUAL_CORE_SHA',core['sha256'])
(D / 'README.md').write_text(readme)
shutil.copy2(Q / 'check-filesystem-validation.py', D / 'check_validation.py')

runtime = R / 'src/user/windows/native-runtime.md'
before = runtime.read_bytes()
manifest = json.loads((Q / 'filesystem-candidate-epoch1.json').read_text())
row = next(row for row in manifest['files'] if row['path'] == 'src/user/windows/native-runtime.md')
assert len(before) == row['bytes'] and hashlib.sha256(before).hexdigest() == row['sha256']
append = (Q / 'runtime-append.pending.md').read_text().replace('ACTUAL_CORE_BYTES',str(core['bytes'])).replace('ACTUAL_CORE_SHA',core['sha256']).encode()
runtime.write_bytes(before + append)
(V / 'engineering-source-updates.json').write_text(json.dumps({
    'path': row['path'], 'before-sha256': row['sha256'], 'before-bytes': row['bytes'],
    'after-sha256': hashlib.sha256(before + append).hexdigest(), 'after-bytes': len(before + append),
    'scope': 'Append-only engineering record after compiled gates; no Rust/test/build/fixture/ABI change.',
}, indent=2) + '\n')
print('Final FileSystem records written; portable proof and exact publication remain.')
