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
R = Q / 'user-source'
D = R / 'docs/specifications/windows/native-token-user'
V = D / 'validation'
assert not (V / 'engineering-source-updates.json').exists(), 'Do not repeat the engineering append.'

owner = json.loads((S / 'user-owner-epoch1-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
archives = json.loads((S / 'native-user-epoch1-final-owning-archive-identities.json').read_text())
core = next(row for row in archives['archives'] if row['path'].endswith('.rlib'))
programs = []
for previous, row in zip([2883974, 2886259], owner['outcomes']):
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
            'scope': 'Both guests return TokenUser success and reach the next unsupported per-user policy registry key. Native controls exit0; controller exit0 does not mean guest success.'}
(Q / 'ordinary-program-summary.json').write_text(json.dumps(ordinary, indent=2) + '\n')
subprocess.run([sys.executable, '-B', str(Q / 'collect-user-evidence.py')], check=True)
subprocess.run([sys.executable, '-B', str(D / 'check_native.py'), str(D)], check=True)
subprocess.run([sys.executable, '-B', str(Q / 'check-user-validation.py'), str(R)], check=True)

plan = json.loads((Q / 'user-plan.json').read_text())
for row in plan['assumptions']:
    if row['id'] == 'A1':
        row['status'] = 'confirmed for recorded native subject and bounded observations'
        row['basis'] += '; native installed leaves independently match SID/attributes and exact retained-owner execution uses that identity.'
    else:
        row['status'] = 'retained: existing guest scratch-budget policy; physical heap exhaustion equivalence is not claimed'
plan['assumptions'].append({'id':'A3','assumption':'SID copies up to68bytes reuse the existing native short-copy plan.',
    'basis':'User12-byte SID boundary captures confirm head/tail order; existing BNO native copy evidence covers the same <=128byte short-copy mechanism.',
    'dependent-result':'Variable SID lengths8..68 preserve the selected converter store plan.',
    'stress-test':'All native User page offsets0..40, writable/read-only/guard combinations; all16SID sizes and opaque attributes in synthetic models.',
    'falsification-probe':'A native TokenUser capture with a different SID length shows different fault-visible store order.',
    'status':'retained: native User matrix has one SYSTEM SID; other SID sizes have synthetic data/layout coverage'})
plan['bounded-scope'][0].update({'item':'Both ordinary guests return TokenUser success and advance2743turns to the per-user Safer policy key without successful guest exit.','evidence':'validation/ordinary-program-summary.json'})
(V / 'final-assumptions.json').write_text(json.dumps({
    'scope': 'TokenUser identity and native ABI semantics; full process objective remains active.',
    'assumptions': plan['assumptions'], 'bounded-scope': plan['bounded-scope'],
    'surface-map': plan['surface-map'],
}, indent=2) + '\n')
quality = {
    'scope':'TokenUser semantic group; full same-host process objective incomplete.',
    'QG1':'pass: factual implementation/verification; no normative judgment',
    'QG2':'pass: bases, dependencies, stress tests and falsification probes; A2/A3 remain declared scope qualifications',
    'QG3':'pass within group: captured SID/attributes, native64/WoW64 probes, typed/query handles, partial copies, aliases, LastError, five shared/six Windows tests, three-host full gates and retained consumers',
    'QG4':'pass: SID8+4*N,N<=15; native64 header16 and WoW64 header8; current required28/20bytes, untouched padding; checked widened scratch arithmetic and exact counts',
    'QG5':'pass within demonstrated scope: full original native/alias/guard data replay, independent native host identity, all previous selectors and ACL interpretation preserved; next registry frontier and scratch/copy limits explicit',
    'QG6':'pass: stored Microsoft sources, native producer/PE/module identities, exact source/archive/program bindings, independent native TEB controls and portable verifiers',
    'QG7':'pass: one physical native kernel, known five lowerer failures, incomplete process objective, retainedABI184/current-root product limitation and inferred scratch/other-SID-copy scope recorded',
}
(V / 'quality-gates.json').write_text(json.dumps(quality, indent=2) + '\n')
native = json.loads((V / 'user-native-epoch1-gates.json').read_text())
(V / 'windows-existing-failures.json').write_text(json.dumps({'failures': native['exact-existing-failures']}, indent=2) + '\n')
summary = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'source-epoch': 1, 'source-inputs': 23442, 'source-bytes': 1695720818,
           'core-counts': {'macos': [8013, 0, 2], 'linux': [8007, 0, 2], 'windows': [7467, 5, 2]},
           'shared-focus': 5, 'windows-focus': 6, 'capi-each': 183,
           'native-cases': 22688, 'native-teb-controls': 36, 'native-negative-controls': 12,
           'owning-selected-inputs': 530, 'ordinary-programs': ordinary,
           'full-process-objective-complete': False}
(V / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')

readme = (Q / 'README.pending.md').read_text().replace('ACTUAL_CORE_BYTES',str(core['bytes'])).replace('ACTUAL_CORE_SHA',core['sha256'])
(D / 'README.md').write_text(readme)
shutil.copy2(Q / 'check-user-validation.py', D / 'check_validation.py')

runtime = R / 'src/user/windows/native-runtime.md'
before = runtime.read_bytes()
manifest = json.loads((Q / 'user-candidate-epoch1.json').read_text())
row = next(row for row in manifest['files'] if row['path'] == 'src/user/windows/native-runtime.md')
assert len(before) == row['bytes'] and hashlib.sha256(before).hexdigest() == row['sha256']
append = (Q / 'runtime-append.pending.md').read_text().replace('ACTUAL_CORE_BYTES',str(core['bytes'])).replace('ACTUAL_CORE_SHA',core['sha256']).encode()
runtime.write_bytes(before + append)
(V / 'engineering-source-updates.json').write_text(json.dumps({
    'path': row['path'], 'before-sha256': row['sha256'], 'before-bytes': row['bytes'],
    'after-sha256': hashlib.sha256(before + append).hexdigest(), 'after-bytes': len(before + append),
    'scope': 'Append-only engineering record after compiled gates; no Rust/test/build/fixture/ABI change.',
}, indent=2) + '\n')
print('Final User records written; portable proof and exact publication remain.')
