from pathlib import Path
import datetime
import hashlib
import json
import shutil
import subprocess
import sys

T = Path(__file__).resolve().parent
R = T / 'timer2-source'
D = R / 'docs/specifications/windows/native-timer2-arm'
V = D / 'validation'
assert not (V / 'engineering-source-updates.json').exists(), 'Inspect prior finalization; do not repeat the append.'
subprocess.run([sys.executable, '-B', str(T / 'collect-timer2-evidence.py')], check=True)
subprocess.run([sys.executable, '-B', str(D / 'check-native-timer2.py'), str(D)], check=True)
subprocess.run([sys.executable, '-B', str(T / 'check-timer2-validation.py'), str(R)], check=True)
plan = json.loads((D / 'plan-and-ownership.json').read_text())
assumptions = plan['assumptions']
for row in assumptions:
    row['status'] = 'confirmed'
    if row['id'] == 'A1':
        row['basis'] += '; all four native constructor/set/cancel profiles, shared dispatcher/HLE state and real installed host/WoW64 leaves agree.'
    elif row['id'] == 'A2':
        row['basis'] += '; native value/fault matrix confirms full16-byte read, ignored Reserved, validation and ABI-specific capture order.'
    else:
        row['basis'] += '; producer handle counts balance and PE/source/runtime identities remain unchanged.'
assumptions.append({'id': 'A4',
    'assumption': 'Relative Timer2 uses the suspend-excluding clock domain documented for modern Windows waitable timers.',
    'basis': 'Stored Microsoft SetWaitableTimerEx Windows8+ clock contract, extended dispatcher model, and the existing three-platform native deadline adapter.',
    'dependent-result': 'Relative Timer2 deadlines and periodic scheduling reuse the native awake-time adapter; UTC absolute targets remain independent.',
    'stress-test': 'Full signed interval boundaries, synthetic UTC adjustments, periodic phase/skip arithmetic, native clock adapters on all hosts.',
    'falsification-probe': 'A controlled native NtSetTimer2 suspend/resume experiment in an isolated machine counts host low-power time against a relative target.',
    'status': 'retained: physical NtSetTimer2 suspend/resume experiment is unknown and is not claimed'})
bounded = plan['bounded-scope']
bounded[0]['item'] = 'Both ordinary programs pass the former timer frontier, advance695677 observer turns, and stop at the uncaptured SafeBoot/Option key without guest exit.'
bounded[0]['evidence'] = 'validation/ordinary-program-summary.json'
bounded.append({'impact': 'medium', 'item': 'Native timer quantum, hardware resolution, power wakeup and physical suspend/resume were not equivalence-tested.',
                'evidence': 'Native timing captures and retained A4 clock-domain inference.', 'blocks-real-time-power-precision-claim': True})
(V / 'final-assumptions.json').write_text(json.dumps({'scope': 'Extended timer semantic group only; complete process objective remains active.', 'assumptions': assumptions, 'bounded-scope': bounded, 'surface-map': plan['surface-map']}, indent=2) + '\n')
quality = {'scope': 'NtSetTimer2/NtCancelTimer2 semantic group; full userland objective remains incomplete.',
    'QG1': 'pass: factual engineering task; no normative judgment needed',
    'QG2': 'pass: explicit bases, dependencies, stress tests and falsification probes; A4 remains a declared inference',
    'QG3': 'pass within group: actual state/expiry/cancel, all ABIs, native leaves, three-host complete selections, actual retained consumers and ordinary progress; full objective not declared complete',
    'QG4': 'pass: full signed64 input widths,100ns units, widened arithmetic, precise source/test counts and recorded timing limits',
    'QG5': 'pass within group: capture ordering/guards, error precedence, rearm/cancel signals, internal pins, wait-packet target release, legacy timers and export stability checked; timing/power limits explicit',
    'QG6': 'pass: pinned PHNT/header license, stored Microsoft documents, native producer/DLL identities and actual source/archive/program bindings',
    'QG7': 'pass: complete-process, current-product, single-physical-kernel, unrelated failures and timing precision limits recorded'}
(V / 'quality-gates.json').write_text(json.dumps(quality, indent=2) + '\n')
native = json.loads((V / 'timer2-native-epoch1-gates.json').read_text())
(V / 'windows-existing-failures.json').write_text(json.dumps({'failures': native['exact-existing-failures']}, indent=2) + '\n')
ordinary = json.loads((V / 'ordinary-program-summary.json').read_text())
core = ordinary['actual-core']
table = '''| Host | Unfiltered core: passed / failed / ignored | New focus | C API | Process integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7,990 / 0 / 2 | 14 passed | 183 passed | 544 passed + 0 cfg cases |
| Linux x86-64 container | 7,984 / 0 / 2 | 14 passed | 183 passed | 544 passed + 0 cfg cases |
| Windows ARM64 | 7,434 / 5 / 2 | 15 passed | 183 passed | 0 cfg cases + 4 native passed |

All three all-target checks pass. Complete core selections are unfiltered.
The exact five retained Windows lowerer failures are recorded in
`validation/windows-existing-failures.json`.'''
paragraph = f'''The smoke program passes the former timer stop at turn 2,188,223 and
reaches turn 2,883,900. `whoami` passes turn 2,190,508 and reaches turn
2,886,185. Both execute another 695,677 observer turns and stop at an uncaptured
`\\Registry\\MACHINE\\System\\CurrentControlSet\\Control\\SafeBoot\\Option`
key. Timer arming returns success at the former frontier. Neither guest exits
successfully. Source, programs, observer executable, core archive, and six
runtime modules are stable before/after. The selected Windows core is
{core['bytes']:,} bytes with SHA-256 `{core['sha256']}`.'''
readme = (T / 'README.pending.md').read_text().replace('FINAL_VALIDATION_TABLE', table).replace('ORDINARY_PROGRAM_RESULTS', paragraph)
assert 'FINAL_VALIDATION_TABLE' not in readme and 'ORDINARY_PROGRAM_RESULTS' not in readme
(D / 'README.md').write_text(readme)
shutil.copy2(T / 'check-timer2-validation.py', D / 'check_validation.py')
runtime = R / 'src/user/windows/native-runtime.md'
before = runtime.read_bytes()
manifest = json.loads((T / 'timer2-source-epoch1.json').read_text())
row = next(r for r in manifest['files'] if r['path'] == 'src/user/windows/native-runtime.md')
assert len(before) == row['bytes'] and hashlib.sha256(before).hexdigest() == row['sha256']
append = f'''

## Native extended timer arming/cancellation (2026-10-19)

NtSetTimer2 and NtCancelTimer2 now operate on the extended objects constructed
by NtCreateTimer2. Native64 and WoW64 input capture order, full16-byte parameter
reads, unaligned spans, guard consumption, modify access, typed handles and
native value restrictions follow18,740 captured cases on four ABI profiles
sharing one Windows ARM64 kernel. Portable replay checks18,528 exact records;
asynchronous timing samples are bounded evidence rather than exact poll goldens.
The independent checker rejects14 corrupted capture copies.

Relative/absolute native clock domains, full signed64 100ns intervals, positive
period phase/skip arithmetic and fast negative periods are retained without
narrowing or overflowing host Instants. At most one expiry is delivered per
scheduler turn. Successful rearm clears signal state; cancellation preserves
it. Existing waits and completion packets consume the same object state and
retain objects after public handle closure. Legacy timers and prior NTDLL
ordinals/trap RVAs are unchanged. Timing quantum/power guarantees and a physical
NtSetTimer2 suspend/resume experiment remain outside the demonstrated scope.

Five strategic tests fail on the missing service before implementation. Final
focus selections pass14 shared tests and15 native Windows tests, including real
installed host/WoW64 NTDLL leaves. Complete unfiltered core pass/fail/ignore is
macOS7990/0/2, Linux7984/0/2, Windows7434/5/2, retaining exactly the established
five Windows lowerer failures. C API183, all-target checks and actual retained
Assist consumers pass on all three hosts. Full archive walks before/after links
bind519 selected inputs. Retained Assist Rust ABI184 and C++ inputs compared
to root ABI264 do not establish whole-current-root GUI/plugin/package builds.
The public C ABI remains1.14.0. The complete source epoch binds23,050 inputs
and1,599,183,887 bytes; this engineering append is separately bound afterward.

Both ordinary Windows programs return timer success at the former stop and
advance695677 normally executed observer turns to an uncaptured SafeBoot/Option
registry key. Neither guest exits successfully; controller exit0 is separate.
The actual Windows owning core is{core['bytes']:,} bytes, SHA-256
`{core['sha256']}`. Sources, captures, logs, commands, assumptions, complete
counts and portable verification are recorded under
`docs/specifications/windows/native-timer2-arm/`.
'''.encode()
runtime.write_bytes(before + append)
update = {'path': row['path'], 'before-sha256': row['sha256'], 'before-bytes': row['bytes'],
    'after-sha256': hashlib.sha256(before + append).hexdigest(), 'after-bytes': len(before + append),
    'scope': 'Append-only engineering record after compiled gates; no Rust/test/build/fixture/ABI change.'}
(V / 'engineering-source-updates.json').write_text(json.dumps(update, indent=2) + '\n')
summary = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'source-epoch': 1,
    'scope': 'Extended timer semantic group complete; full process objective incomplete',
    'core-counts': {'macos': [7990, 0, 2], 'linux': [7984, 0, 2], 'windows': [7434, 5, 2]},
    'new-shared-tests': 14, 'new-native-tests': 15, 'capi-tests-each-host': 183,
    'native-cases': 18740, 'native-replayed-records': 18528, 'native-negative-controls': 14,
    'owning-selected-inputs': 519, 'ordinary-programs': ordinary}
(V / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print('Final engineering records written; complete portable proof and exact owned publication next.')
