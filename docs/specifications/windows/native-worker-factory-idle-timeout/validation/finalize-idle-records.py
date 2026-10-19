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
R = Q / 'idle-source-epoch2'
D = R / 'docs/specifications/windows/native-worker-factory-idle-timeout'
V = D / 'validation'
assert not (V / 'engineering-source-updates.json').exists(), 'Do not repeat the engineering append.'

owner = json.loads((S / 'idle-owner-epoch2-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
archives = json.loads((S / 'native-idle-epoch2-final-owning-archive-identities.json').read_text())
core = next(row for row in archives['archives'] if row['path'].endswith('.rlib'))
programs = []
for previous, row in zip([2898814, 2901099], owner['outcomes']):
    raw = (S / row['log']).read_bytes()
    match = re.search(r'^terminal turn=(\d+) (.+)$', raw.decode(), re.M)
    assert match and 'NtQuerySecurityAttributesToken (0x167)' in match.group(2) and 'is not implemented' in match.group(2)
    programs.append({'program': row['program-before'], 'log': row['log'],
                     'log-sha256': hashlib.sha256(raw).hexdigest(),
                     'controller-exit-code': row['exit-code'],
                     'former-stop-turn': previous, 'new-terminal-turn': int(match.group(1)),
                     'additional-observer-turns': int(match.group(1)) - previous,
                     'terminal-result': match.group(2), 'guest-exit-success': False,
                     'native-before': row['native-before'], 'native-after': row['native-after']})
ordinary = {'actual-core': core, 'programs': programs, 'ordinary-guest-exit-success': False,
            'scope': 'Both guests return class2 success and retain normalized -300000000 ticks, then advance359 turns to unsupported NtQuerySecurityAttributesToken. Native controls exit0; controller exit0 does not mean guest success.'}
(Q / 'ordinary-program-summary.json').write_text(json.dumps(ordinary, indent=2) + '\n')
shutil.copy2(Q / 'idle-native-negative-controls.json', D / 'idle-native-negative-controls.json')
subprocess.run([sys.executable, '-B', str(Q / 'collect-idle-evidence.py')], check=True)
subprocess.run([sys.executable, '-B', str(D / 'check_native.py'), str(D)], check=True)
subprocess.run([sys.executable, '-B', str(Q / 'check-idle-validation.py'), str(R)], check=True)


plan = json.loads((Q / 'idle-plan.json').read_text())
for assumption in plan['assumptions']:
    if assumption['id'] == 'A1':
        assumption['status'] = 'confirmed within one installed kernel and dormant contexts; other builds and active scheduling remain unknown'
        assumption['basis'] += '; 8944 checked native observations, exact replay on all three hosts and actual owning class2 success.'
    elif assumption['id'] == 'A2':
        assumption['status'] = 'retained: None records no request; constructor policy outside measured context remains unknown'
    elif assumption['id'] == 'A3':
        assumption['status'] = 'confirmed: cross-page replay reproduces native status and each page protection, including first reached guard consumption'
plan['bounded-findings'][0].update({
    'item': 'Both ordinary Windows programs advance359turns after class2 success to unimplemented NtQuerySecurityAttributesToken; neither exits successfully.',
    'evidence': 'validation/ordinary-program-summary.json', 'blocks-full-goal': True,
})
plan['bounded-findings'].append({
    'impact': 'medium', 'item': 'Retained Assist Rust ABI184 and selected C++ consumers cover the linked process engine; whole current-root GUI/plugin/package validation is not established by these retained consumers.',
    'evidence': 'validation/idle-owning-source-hashes-epoch2.json', 'blocks-this-semantic-group': False,
})
(V / 'final-assumptions.json').write_text(json.dumps({
    'scope': 'Dormant WorkerFactoryIdleTimeout semantic group; complete process objective remains active.',
    'assumptions': plan['assumptions'], 'bounded-findings': plan['bounded-findings'], 'surface-map': plan['surface-map'],
}, indent=2) + '\n')

counts = {'macos': [8037, 0, 2], 'linux': [8031, 0, 2], 'windows': [7499, 5, 2]}
validation_table = '| Host | Passed | Failed | Ignored |\n|---|---:|---:|---:|\n'
for host, values in counts.items():
    validation_table += '| ' + host + ' | ' + ' | '.join(str(value) for value in values) + ' |\n'
validation_table += '\nThe Windows failures match the exact five pre-existing lowerer failures. Each host also passed 183 C API tests; integration selections passed 544 on macOS/Linux and 4 on Windows, with cfg-empty counterpart binaries identified as such. All-target compilation passed. Retained C++ selections passed 7 CTests on macOS and 5 consumers each on Linux/Windows, with six actual macOS target link commands and protected CLI/MCP scans checked.'
inline = 'Complete library selections: macOS8037pass/0fail/2ignored, Linux8031/0/2, Windows7499/5/2 with exactly the five established lowerer failures. C API183 each, integration544 on macOS/Linux and4 on Windows, all-target compilation and selected retained consumers passed.'
result = 'Both ordinary ARM64 guests return class2 status0 and retain idle_timeout Some(-300000000), then advance359turns: smoke2898814->2899173 and whoami2901099->2901458. The next stop is unimplemented NtQuerySecurityAttributesToken (0x167). Neither guest exits successfully; both native controls exit0. Read-only final diagnostics capture pseudo token handle -4 and requested WIN://SYSAPPID/WIN://PKG names.'

def render(text):
    return text.replace('ACTUAL_CORE_BYTES', str(core['bytes'])).replace('ACTUAL_CORE_SHA', core['sha256']).replace('ACTUAL_VALIDATION_TABLE', validation_table).replace('ACTUAL_VALIDATION_INLINE', inline).replace('ACTUAL_ORDINARY_RESULT', result)

(D / 'README.md').write_text(render((Q / 'README.pending.md').read_text()))
shutil.copy2(Q / 'check-idle-validation.py', D / 'check_validation.py')
runtime = R / 'src/user/windows/native-runtime.md'
before = runtime.read_bytes()
manifest = json.loads((Q / 'idle-candidate-epoch2.json').read_text())
source_row = next(row for row in manifest['files'] if row['path'] == 'src/user/windows/native-runtime.md')
assert len(before) == source_row['bytes'] and hashlib.sha256(before).hexdigest() == source_row['sha256']
addition = render((Q / 'runtime-append.pending.md').read_text()).encode()
runtime.write_bytes(before + addition)
(V / 'engineering-source-updates.json').write_text(json.dumps({
    'path': source_row['path'], 'before-sha256': source_row['sha256'], 'before-bytes': source_row['bytes'],
    'after-sha256': hashlib.sha256(before + addition).hexdigest(), 'after-bytes': len(before + addition),
    'scope': 'Append-only engineering record after compiled gates. No Rust/test/build/fixture/ABI input changes.',
}, indent=2) + '\n')
subprocess.run([sys.executable, '-B', str(Q / 'stress-idle-validation.py')], check=True)
controls = json.loads((Q / 'idle-final-negative-controls.json').read_text())
assert controls['all-rejected'] and len(controls['controls']) == 17
shutil.copy2(Q / 'idle-final-negative-controls.json', V / 'idle-final-negative-controls.json')
quality = {
    'scope': 'Dormant class2 semantic group. Full same-host process objective remains incomplete.',
    'QG1': 'pass: factual implementation and validation; no normative content',
    'QG2': 'pass: A1 scoped to captured kernel/dormant contexts, A2 constructor unknown retained, A3 native/shared cross-page stress confirmed',
    'QG3': 'pass within group: native64/WoW lengths, 4-byte alignment, 8-byte capture, type/access/value priority, clamping, aliases, failed-call atomicity, unchanged adjacent class state and native leaves',
    'QG4': 'pass: signed100ns ticks converted exactly to relative1..600s; i64::MIN never negated; raw input/output counts and ABI/x86return20bytes verified',
    'QG5': 'pass within group: genuine compiled baseline0pass/3fail; six shared/seven Windows tests pass; initial native test import failure preserved and corrected in immutable epoch2; incomplete process exit explicit',
    'QG6': 'pass: pinned PHNT declaration/license, source/PE/runtime identities,8944native observations,14native and17validation corruptions rejected, full source/real archive/program/log binding and portable checker',
    'QG7': 'pass: one kernel and dormant scope, constructor/active unknowns, known five lowerer failures, retained/current-product limit and new token-attribute frontier recorded',
    'RAX-QG8': 'pass: all8944native rows replayed on shared paths, installed native/WoW leaves exercised and full target selections run; actual owning status/state/359turn advance proved',
    'RAX-QG9': 'pass: all constructors/exhaustive matches/tests/source membership updated; public Rust field change documented and opaque CAPI1.14/defaults/pins unchanged; engineering append separately bound',
}
(V / 'quality-gates.json').write_text(json.dumps(quality, indent=2) + '\n')
native_record = json.loads((V / 'idle-native-epoch2-gates.json').read_text())
(V / 'windows-existing-failures.json').write_text(json.dumps({'failures': native_record['exact-existing-failures']}, indent=2) + '\n')
summary_record = {
    'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'source-epoch': 2,
    'source-inputs': 24313, 'source-bytes': 1752724333, 'core-counts': counts,
    'shared-focus': 6, 'windows-focus': 7, 'capi-each': 183, 'native-observations': 8944,
    'native-negative-controls': 14, 'validation-negative-controls': 17,
    'owning-selected-inputs': 554, 'ordinary-programs': ordinary, 'full-process-objective-complete': False,
}
(V / 'summary.json').write_text(json.dumps(summary_record, indent=2) + '\n')
print('Final dormant idle-timeout records complete; portable proof and exact publication remain.')
