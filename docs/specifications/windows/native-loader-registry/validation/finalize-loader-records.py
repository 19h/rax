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
R = Q / 'loader-source'
D = R / 'docs/specifications/windows/native-loader-registry'
V = D / 'validation'
assert not (V / 'engineering-source-updates.json').exists(), 'Do not repeat the engineering append.'

owner = json.loads((S / 'loader-owner-epoch3-launch.json').read_text())
assert owner['state'] == 'finished' and owner['exit_code'] == 0
archives = json.loads((S / 'native-loader-epoch3-final-owning-archive-identities.json').read_text())
core = next(row for row in archives['archives'] if row['path'].endswith('.rlib'))
programs = []
for previous, row in zip([2883900, 2886185], owner['outcomes']):
    raw = (S / row['log']).read_bytes()
    match = re.search(r'^terminal turn=(\d+) (.+)$', raw.decode(), re.M)
    assert match and 'NtQueryInformationToken class 1' in match.group(2)
    programs.append({'program': row['program-before'], 'log': row['log'],
                     'log-sha256': hashlib.sha256(raw).hexdigest(),
                     'controller-exit-code': row['exit-code'],
                     'former-stop-turn': previous, 'new-terminal-turn': int(match.group(1)),
                     'additional-observer-turns': int(match.group(1)) - previous,
                     'terminal-result': match.group(2), 'guest-exit-success': False,
                     'native-before': row['native-before'], 'native-after': row['native-after']})
ordinary = {'actual-core': core, 'programs': programs, 'ordinary-guest-exit-success': False,
            'scope': 'Both guests pass all three fixed registry roots and reach the next unsupported token query. Native controls exit0; controller exit0 does not mean guest success.'}
(Q / 'ordinary-program-summary.json').write_text(json.dumps(ordinary, indent=2) + '\n')
subprocess.run([sys.executable, '-B', str(Q / 'collect-loader-evidence.py')], check=True)
subprocess.run([sys.executable, '-B', str(D / 'check_native.py'), str(D)], check=True)
subprocess.run([sys.executable, '-B', str(Q / 'check-loader-validation.py'), str(R)], check=True)

plan = json.loads((Q / 'loader-plan.json').read_text())
for row in plan['assumptions']:
    row['status'] = 'confirmed for the recorded source, native subject and bounded observations'
    row['basis'] += '; final three-host checks and exact retained-owner observations agree.'
(V / 'final-assumptions.json').write_text(json.dumps({
    'scope': 'Three fixed immutable loader registry roots; full process objective remains active.',
    'assumptions': plan['assumptions'], 'bounded-scope': plan['bounded-scope'],
    'surface-map': plan['surface-map'],
}, indent=2) + '\n')
quality = {
    'scope': 'Fixed loader registry semantic group only; complete process objective is incomplete.',
    'QG1': 'pass: factual implementation and validation, no normative judgment',
    'QG2': 'pass: bases, dependencies, stress tests and falsification probes recorded',
    'QG3': 'pass within group: three observed roots, eighteen shared/nine additional native tests, full three-host core/CAPI/integration/all-target checks and actual retained consumers',
    'QG4': 'pass: UTF16 byte/unit boundaries and fixed capture/value budgets preserved; 2samples*8paths*5rights*4profiles*3roots=960cases',
    'QG5': 'pass within group: absence versus unselected, opaque values, root/child errors, aliases, shared budgets, read-only grants and handle lifetimes checked; full-process token frontier explicit',
    'QG6': 'pass: native source/PE/module/program/archive bindings, saved Microsoft source, full source snapshot and portable checkers',
    'QG7': 'pass: immutable registry limitation, one physical native kernel, five established lowerer failures, retained ABI184 consumers and next unsupported service documented',
}
(V / 'quality-gates.json').write_text(json.dumps(quality, indent=2) + '\n')
native = json.loads((V / 'loader-native-epoch3b-gates.json').read_text())
(V / 'windows-existing-failures.json').write_text(json.dumps({'failures': native['exact-existing-failures']}, indent=2) + '\n')
summary = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'source-epoch': 3, 'source-inputs': 23283, 'source-bytes': 1657974640,
           'core-counts': {'macos': [8008, 0, 2], 'linux': [8002, 0, 2], 'windows': [7461, 5, 2]},
           'shared-focus': 18, 'windows-focus': 27, 'capi-each': 183,
           'native-cases': 960, 'native-negative-controls': 18,
           'owning-selected-inputs': 528, 'ordinary-programs': ordinary,
           'full-process-objective-complete': False}
(V / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')

readme = f'''# Fixed native loader registry snapshots

Installed Windows runtime selection now captures three additional fixed registry
roots through the existing complete-or-known-absent immutable tree adapter.
These roots were observed in ordinary execution, in this order:

| Native root below `\\Registry\\Machine` | Observed desired access | Captured native root |
| --- | --- | --- |
| `System\\CurrentControlSet\\Control\\SafeBoot\\Option` | `0x3` | absent |
| `System\\CurrentControlSet\\Control\\Srp\\GP\\DLL` | `0x20019` | absent |
| `Software\\Policies\\Microsoft\\Windows\\Safer\\CodeIdentifiers` | `0x1` | present |

The existing IFEO and StateSeparation roots remain selected. Root-only Win32
`ERROR_FILE_NOT_FOUND` establishes absence; other acquisition errors, unstable
values or disappearing enumerated children abort capture. No guest path reaches
a native registry opener. Unknown parents and siblings retain the unsupported
selection boundary. Present trees own raw typed value bytes and descendants;
guest handles retain that ownership after the catalog is dropped.

The inherited guest registry contract is read-only. Write-intent access to a
present tree is denied even when the captured host token could write it. Native
Safer access `0x3` succeeds on this subject; that observation is retained as a
reference and does not broaden guest permissions. The ordinary Safer request
uses `0x1`. Both observed SYSTEM requests reach known absence before grants.
This change does not establish general mutable registry parity.

Tree metadata limits remain1024keys,maximum descendant depth32(rootdepth0),
255UTF16units per child component,
32767UTF16units per path and1048576aggregate path bytes per selected root.
Existing value count/byte limits remain shared across all selected roots.
For Kkeys,Vvalues,Bcopied bytes, construction uses
O((K+V+B)log(K+V+2)) local work and O(K+V+B) retained storage,
excluding native API internal costs; the number of selected fixed roots is5.
Existing three-attempt per-key stability checks and ordinal case folding remain
unchanged. Registry names use their own separator grammar, independent of host
filesystem paths. The [Microsoft registry view contract](https://learn.microsoft.com/en-us/windows/win32/winprog64/shared-registry-keys)
is stored with retrieval time and SHA256; native view probes independently agree.

## Native reference and validation

Three producers record2samples*8paths*5rights*4ABIprofiles=960cases. Each profile
contains80cases per root.840observations address the selected roots and120are
outside-prefix controls. SafeBoot and SRP each return320name-not-found results;
Safer returns120successful root/alias opens and200missing descendant/control
results. Success handles are opaque and nonzero, never numeric goldens.
Native source, PE machine/LAA status, process handle counts and six installed
runtime module identities are checked before/after. The four ABI profiles
(ARM64,x64compatibility,x86,x86LAA) share one Windows10.0.29683 ARM64 kernel.
The native checker rejects18corrupted copies.

| Host | Unfiltered core passed/failed/ignored | New focused tests | C API | Process integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 8008/0/2 | 18 | 183 | 544 plus0cfg cases |
| Linux x86-64 container | 8002/0/2 | 18 | 183 | 544 plus0cfg cases |
| Windows ARM64 | 7461/5/2 | 27 | 183 | 0cfg cases plus4native |

All three all-target checks pass. The five Windows lowerer failures match the
exact established names in `validation/windows-existing-failures.json`.
A tests-only baseline compiles, then fails its actual native absence assertion
on the prior implementation (0passed/1failed). All Rust changes preceded the
final full gates. The complete source epoch binds23283inputs/1657974640bytes.
The post-gate runtime engineering append is separately identified.

Actual retained Assist core/carrier builds and selected C++ consumers pass on
all three hosts, binding528selected source inputs and full archive walks before
and after C++ links. macOS also passes seven exact CTests and actual CLI/MCP
carrier linkage/protected-text checks. The retained Assist Rust ABI is184;
eight C++ inputs were previously compared with root ABI264. These results do
not establish whole-current-root GUI/plugin/package coverage. Public RAX C API
remains1.14.0; defaults, dependencies and public layouts are unchanged.

## Ordinary execution and limits

The smoke guest passes the old turn2883900 and reaches2883974; whoami passes
2886185 and reaches2886259. Both execute another74observer turns normally,
return C0000034 for both absent SYSTEM roots, successfully open Safer, and stop
at unsupported NtQueryInformationToken class1. Neither guest exits successfully.
Both native controls exit0; native whoami emits `nt authority\\system` followed
by CRLF. Guest stdout/stderr remain empty at this startup frontier.

The exact validated retained-owner core is{core['bytes']}bytes, SHA256
`{core['sha256']}`. The observer links this actual archive, executes the prefix
normally and binds source/executable/program/six-module/archive identities
before and after. Controller success is distinct from guest success.

`validation/loader-plan.json` and `validation/final-assumptions.json` own the
final scope. Earlier SafeBoot-only and two-root plans/candidates are historical
exploration evidence. The first final native driver failed before invoking a
compiler because its preparation-helper name was stale; the separately named
epoch3b driver verifies the already materialized immutable source and completes
the full gates. The Safer controlled-present test has an inherited srp_dll
diagnostic label; its actual test identity, selected root and assertions are
Safer. No inference relies on that diagnostic text.

Run `python3 -B check_native.py .` here and
`python3 -B check_validation.py /path/to/rax` from any supported host.
The evidence manifest binds every evidence file except itself. Three-host
verification receipts record their original checked-envelope scope; later
receipts and the verbatim whitespace audit are explicitly added afterward.
The complete same-host process objective remains incomplete.
'''
(D / 'README.md').write_text(readme)
shutil.copy2(Q / 'check-loader-validation.py', D / 'check_validation.py')

runtime = R / 'src/user/windows/native-runtime.md'
before = runtime.read_bytes()
manifest = json.loads((Q / 'loader-candidate-epoch3.json').read_text())
row = next(row for row in manifest['files'] if row['path'] == 'src/user/windows/native-runtime.md')
assert len(before) == row['bytes'] and hashlib.sha256(before).hexdigest() == row['sha256']
append = f'''

## Fixed loader registry roots (2026-10-19)

Native selection additionally captures SafeBoot/Option, Srp/GP/DLL and
Safer/CodeIdentifiers as complete-or-known-absent immutable trees. All three
roots were observed during ordinary Windows execution. Existing per-root tree
bounds, aggregate value budgets, ordinal folding, raw values and retained key
handle ownership remain unchanged. Only fixed-root ERROR_FILE_NOT_FOUND means
absence; other root errors or disappearing enumerated children abort capture.
Guest queries remain read-only; host write permission is not forwarded.

Three independent native capture families bind960cases across four ABI profiles
on one ARM64 kernel;18corrupted capture copies are rejected. SafeBoot and SRP
are absent on this subject; Safer is present. The product dynamically captures
present or absent state. Eighteen shared and27total Windows focused tests pass.
Full unfiltered core pass/fail/ignore is macOS8008/0/2,Linux8002/0/2,
Windows7461/5/2, retaining the exact five established Windows lowerer failures.
CAPI183, process integration and all-target checks pass across all three hosts.
Actual retained Assist core/carrier archives and C++ consumers bind528selected
inputs with complete archive walks before/after links. The retained Rust ABI184
and C++ input comparison to root ABI264 do not establish whole-current-root
GUI/plugin/package coverage. Public RAX C API remains1.14.0.

Both ordinary Windows guests pass all three roots and advance74normally
executed observer turns to unsupported NtQueryInformationToken class1, without
successful guest exit. Native controls exit0 with stable output. The exact
validated owning core is{core['bytes']}bytes, SHA256
`{core['sha256']}`. Sources, executable, modules, programs and archive remain
stable. The complete pre-gate epoch binds23283inputs/1657974640bytes; this
engineering append is separately bound. Sources, assumptions, known limits,
complete counts and portable verification are under
`docs/specifications/windows/native-loader-registry/`.
'''.encode()
runtime.write_bytes(before + append)
(V / 'engineering-source-updates.json').write_text(json.dumps({
    'path': row['path'], 'before-sha256': row['sha256'], 'before-bytes': row['bytes'],
    'after-sha256': hashlib.sha256(before + append).hexdigest(), 'after-bytes': len(before + append),
    'scope': 'Append-only engineering record after compiled gates; no Rust/test/build/fixture/ABI change.',
}, indent=2) + '\n')
print('Final loader records written; portable proof and exact publication remain.')
