"""Verify the selected final source, native/Unix gates, and retained carriers."""
from pathlib import Path
import argparse
import hashlib
import json
import re

D = Path(__file__).resolve().parent
V = D / 'validation'
parser = argparse.ArgumentParser()
parser.add_argument('--source-root', type=Path, default=D.parents[3])
P = parser.parse_args().source_root


def need(ok, message):
    if not ok:
        raise ValueError(message)


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source(row):
    raw = (P / row['path']).read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
         'final source ' + row['path'])


def counts(name):
    return [tuple(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',
        (V / name).read_text(encoding='utf-8-sig'))]


need(__debug__, 'optimized validation rejected')
need((D / '.gitattributes').read_text() == '# Preserve raw native proof bytes on every host.\n* -text\n',
     'proof line-ending attributes')
for row in load(D / 'line-ending-verification.json')['files']:
    raw = (D / row['path']).read_bytes()
    blob = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
    need(row['equal'] and row['raw-git-blob'] == row['autocrlf-true-git-blob'] == blob,
         'line-ending preservation ' + row['path'])

seen = set()
for row in load(V / 'inputs.json')['files']:
    need(row['path'] not in seen, 'duplicate evidence path')
    seen.add(row['path'])
    raw = (D / row['path']).read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
         'evidence identity ' + row['path'])

manifest = load(V / 'base-final-source.json')
need(manifest['base'] == 'be05c4b474dbf76be910e0f50dbc4be3262c16ce', 'source parent')
need(manifest.get('epoch') == 2, 'partial-capture source revision')
inputs = manifest['files']
need(len(inputs) == len({r['path'] for r in inputs}) == 5728, 'selected final source inputs')
for row in inputs:
    source(row)

upstream = load(V / 'upstream-cpp-consumer-compatibility.json')
compared = load(V / 'current-cpp-consumers-base-root.json')
need(upstream['files'] == compared['files'] and len(upstream['files']) == 9,
     'preserved upstream C++ source identities')
need(upstream['compared-root'] == compared['current-root-head'] and
     upstream['frozen-carrier-abi-minor'] == 184 and upstream['upstream-abi-minor'] >= 200,
     'retained carrier versus upstream distinction')

recovery = load(V / 'namespace-storage-recovery.json')
need(recovery['state'] == 'finished' and recovery['exit-code'] == 0
     and recovery['source-epoch'] == 2, 'completed Windows storage recovery')
need(recovery['original-driver']['state'] == 'failed'
     and recovery['original-driver']['core-exit-code'] == 101,
     'original failed driver retained')
need(recovery['environment'] == {'CARGO_INCREMENTAL': '0'},
     'only recovered check disables incremental caching')
need(len(recovery['checks']) == 3 and all(r['exit-code'] == 0 for r in recovery['checks']),
     'successful recovered command and source checks')
cleanup = load(V / 'namespace-storage-cleanup.json')
need(cleanup['free-after'] > cleanup['free-before'] and cleanup['files'] > 0
     and cleanup['deleted-directory'].replace('\\', '/').endswith('/target-system-time-final/debug/incremental'),
     'scoped inactive incremental-cache cleanup')
need('os error 112' in (V / 'development/storage-failure/native-base-final-all-targets.log').read_text(encoding='utf-8-sig'),
     'original insufficient-storage failure retained')
for name in ['namespace-recovery-source-before.log', 'namespace-recovery-source-after.log']:
    need('Verified final native base-directory source inputs: 5728' in
         (V / name).read_text(encoding='utf-8-sig'), 'recovery source identity check')

completion = load(V / 'base-final-post-run-completion.json')
need([(r['script'], r['exit-code']) for r in completion['checks']] ==
     [('verify-base-final.py', 0), ('verify-base-post-run.py', 0)],
     'post-completion native source and artifact verification')
need('Verified final native base-directory source inputs: 5728' in
     (V / 'native-base-final-source-post-run.log').read_text(encoding='utf-8-sig'),
     'post-completion complete selected source check')

post_run = {r['path'].replace('\\', '/').split('/')[-1]: r
            for r in load(V / 'native-base-final-post-run-identities.json')}
for name, expected in {
    'native-shared-section-trace.rs': '514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0',
    'config.toml': '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c',
    'rax_process.cpp': 'ed55d78681a9a59e384186382d998244d80ca5665793e60f5e7577d88d526564',
    'ntdll.dll': '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'whoami.exe': '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6',
    'smoke.exe': '325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
}.items():
    need(post_run[name]['sha256'] == expected, 'post-run identity ' + name)
need(post_run['native-base-final-trace.exe']['bytes'] > 0,
     'post-run ordinary observer executable')

ignored = {'smir::lower::validation::tests::test_lift_lower_full_microkernel',
           'smir::lower::validation::tests::test_roundtrip_exact_bytes_microkernel'}
baseline_failures = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}
for host, total, failures in [('macos', 7921, 0), ('linux', 7915, 0), ('native', 7358, 5)]:
    name = host + '-base-final-full.log'
    text = (V / name).read_text(encoding='utf-8-sig')
    need(counts(name) == [(total, failures, 2, 0)], 'full test counts ' + host)
    need(set(re.findall(r'^test (\S+) \.\.\. ignored$', text, re.M)) == ignored,
         'ignored test identities ' + host)
    if failures:
        failed = set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))
        need(failed == baseline_failures, 'native baseline failure identities')
    gates = load(V / (host + '-base-final-gates.json'))
    need([r['gate'] for r in gates] == (
        ['integration', 'capi', 'full', 'all-targets'] if host == 'native'
        else ['full', 'capi', 'integration', 'all-targets']), 'gate selection ' + host)
    for row in gates:
        command = ' '.join(row['argv']) if 'argv' in row else row['command']
        need(all(flag in command for flag in ['--locked', '--offline', '--no-default-features']),
             'core configuration ' + host + '/' + row['gate'])
        selection = {'full': ['--lib'], 'capi': ['-p rax-capi'],
                     'integration': ['--test user_windows', '--test user_windows_memory'],
                     'all-targets': ['check', '--all-targets']}[row['gate']]
        need(all(flag in command for flag in selection), 'core target selection')
        expected = 101 if host == 'native' and row['gate'] == 'full' else 0
        need(row['exit-code'] == expected, 'gate exit ' + host + '/' + row['gate'])
        if 'sha256' in row:
            need(digest(V / row['log']) == row['sha256'], 'gate log identity')
    need((168, 0, 0, 0) in counts(host + '-base-final-capi.log'), 'C API count ' + host)
    expected = [(0, 0, 0, 0), (4, 0, 0, 0)] if host == 'native' else [
        (544, 0, 0, 0), (0, 0, 0, 0)]
    need(counts(host + '-base-final-integration.log') == expected,
         'integration selection ' + host)

for host, log, passed, filtered in [
    ('macos', 'partial-macos-focus.log', 12, 7911),
    ('linux', 'partial-linux-focus.log', 12, 7905),
    ('native', 'partial-native-focus.log', 13, 7352),
]:
    need(counts(log) == [(passed, 0, 0, filtered)], 'focused namespace result ' + host)
port = load(V / 'real-worktree-port.json')
need(port['base'] == 'be05c4b474dbf76be910e0f50dbc4be3262c16ce' and
     port['source-inputs'] == 5728, 'prototype/native port identity')

for name in ['base-final-launch.json', 'base-final-owned-launch.json']:
    row = load(V / name)
    need(row['state'] == 'finished' and row['exit-code'] == 0, 'native driver completion')

for host, expected in [('macos', 7), ('linux', 4)]:
    gates = load(V / (host + '-base-owning-gates.json'))
    need(len(gates) == expected, 'owning gate count ' + host)
    for row in gates:
        need(row['exit-code'] == 0 and digest(V / row['log']) == row['sha256'],
             'owning gate and log ' + host)

for host in ['macos', 'linux', 'native']:
    archive = load(V / (host + '-base-final-owning-archive-identities.json'))
    need(archive['source-epoch'] == 'native-base-directories-final', 'owning source epoch')
    metadata = archive['compiler-messages']
    name = metadata['path'].replace('\\', '/').split('/')[-1]
    raw = (V / name).read_bytes()
    need(len(raw) == metadata['bytes'] and hashlib.sha256(raw).hexdigest() == metadata['sha256'],
         'owning Cargo message stream ' + host)
    messages = []
    for line in raw.decode('utf-8-sig').splitlines():
        try:
            messages.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    need(archive['core-artifact'] in messages and archive['carrier-artifact'] in messages,
         'actual Cargo-selected artifact records ' + host)
    need(archive['core-features'] == [] and archive['core-artifact']['features'] == [],
         'empty-feature core ' + host)
    need(len(archive['archives']) == 2 and len(archive['sources']) == 419,
         'owning archives and selected sources ' + host)
    for row in archive['sources']:
        source(row)
    for row in archive['archives']:
        need(row['structural-member-walk'] == 'pass' and row['members'] > 0
             and row['bytes'] > 0 and len(row['sha256']) == 64, 'complete archive walk')
    filenames = archive['core-artifact']['filenames'] + archive['carrier-artifact']['filenames']
    need(all(row['path'] in filenames for row in archive['archives']), 'actual Cargo artifacts')

ctest = (V / 'macos-base-owning-ctest.log').read_text()
passed = re.findall(r'^\d+/7 Test #\d+: (\S+)\s+\.+\s+Passed', ctest, re.M)
need(len(passed) == 7 and set(passed) == {
    'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
    'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
    'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test',
}, 'macOS seven CTest outcomes')
for host, name, process_checks in [('native', 'assist-native-base-final-cpp.log', 182),
                                    ('linux', 'linux-base-owning-cpp.log', 184)]:
    text = (V / name).read_text(encoding='utf-8-sig')
    for token in [str(process_checks) + ' checks passed', '162 checks passed', '2/2 passed',
                  'PASS', 'canonical discovery and explicit RAX-disabled refusal passed']:
        need(token in text, 'C++ consumer ' + host + '/' + token)

ordinary = load(V / 'ordinary-analysis.json')
contract = load(V / 'ordinary-contract.json')
executed = [r for r in ordinary if 'terminal-lines' in r]
need(len(executed) == len(contract) == 2, 'ordinary execution count')
for index, (row, expected) in enumerate(zip(executed, contract)):
    need(row['controller-exit-code'] == 0 and row['program-sha256'] == expected['program-sha256'],
         'ordinary controller and target identity')
    need(len(row['terminal-lines']) == 1 and row['terminal-lines'][0] == expected['terminal'],
         'ordinary measured terminal outcome')
    need(digest(V / row['log']) == row['sha256'], 'ordinary log identity')
    text = (V / row['log']).read_text(encoding='utf-8-sig')
    open_turn, query_turn = [(2149659, 2149788), (2152008, 2152137)][index]
    need(f'kernel-before turn={open_turn} service=0x133' in text and
         f'kernel-after turn={open_turn} service=0x133 PC=0x7ff82fad2374 X0=0x0' in text,
         'ordinary successful token opening')
    need(f'kernel-before turn={query_turn} service=0x21' in text and
         f'kernel-after turn={query_turn} service=0x21' in text,
         'ordinary class12 call returned')
    need(f'kernel-after turn={query_turn} service=0x21 PC=0x7ff82fad1224 X0=0x0' in text,
         'ordinary class12 successful return')
    need(f'kernel-after turn={query_turn + 14} service=0x21 PC=0x7ff82fad1224 X0=0x0' in text,
         'ordinary class29 remains successful')
    need(f'kernel-after turn={query_turn + 27} service=0x21 PC=0x7ff82fad1224 X0=0x0' in text,
         'ordinary previous class42 boundary returned successfully')
    directory_turn, terminal_turn = [(2149956, 2149970), (2152305, 2152319)][index]
    need(f'kernel-before turn={directory_turn} service=0x58' in text and
         f'kernel-after turn={directory_turn} service=0x58 PC=0x7ff82fad15c4 X0=0x0' in text,
         'ordinary base directory opening returned success')
    terminal = f'terminal turn={terminal_turn} ' + 'Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\\"NtQueryInformationToken: NtQueryInformationToken class 44\\")"))'
    need(row['terminal-lines'] == [terminal] and expected['terminal'] == terminal,
         'measured class44 boundary; controller zero is not a guest exit')

owner = load(V / 'base-owner-observer-launch.json')
need(owner['state'] == 'finished' and owner['exit_code'] == 0, 'owner diagnostic completed')
for label in ['source', 'executable', 'archive']:
    before = owner[label + '-before'] if label != 'archive' else owner['archive']
    after = owner[label + '-after']
    need(before['sha256'] == after['sha256'] and before['bytes'] == after['bytes'],
         'owner diagnostic before/after identity ' + label)
need(owner['source-before']['sha256'] ==
     '1a5d047c324cdcc365d697273bacca64536d0c4366ee58dc0e66764119efa907',
     'unchanged fast diagnostic source')
need(digest(V / 'owner-diagnostic/base-owner-observer.rs') == owner['source-before']['sha256'],
     'retained diagnostic source bytes')
archives = load(V / 'native-base-final-owning-archive-identities.json')['archives']
need(any(row['sha256'] == owner['archive']['sha256'] and row['path'] == owner['archive']['path']
         for row in archives), 'diagnostic linked to selected retained-owner archive')
need(len(owner['outcomes']) == 2, 'two owner diagnostic targets')
for index, row in enumerate(owner['outcomes']):
    need(row['exit-code'] == 0 and row['program-before'] == row['program-after'],
         'owner diagnostic target unchanged')
    need(row['program-before']['sha256'] == contract[index]['program-sha256'],
         'owner diagnostic same target')
    text = (V / row['log']).read_text(encoding='utf-8-sig')
    need(digest(V / row['log']) == row['log-identity']['sha256'], 'owner diagnostic log identity')
    need('prefix-turns=2100000 result=BudgetExhausted' in text and
         contract[index]['terminal'] in text, 'owner diagnostic same measured terminal')

need(digest(V / 'native-namespace-churn.rs') ==
     '507fda0612ad1a9a39c888bc55c0ba58b37dd07524eccf0eb83e219228ba8eb9',
     'Revised churn identical source')
for kind in ['archive', 'exe']:
    before = load(V / ('namespace-churn-new-' + kind + '-before.json'))
    after = load(V / ('namespace-churn-new-' + kind + '-after.json'))
    need(before == after, 'Revised churn stable artifact ' + kind)
churn = load(V / 'namespace-churn-new-run.json')
need(churn['state'] == 'finished' and churn['exit-code'] == 0 and churn['source-epoch'] == 2,
     'Revised churn completed with current source epoch')
need(churn['cargo-artifact']['features'] == [] and
     churn['artifact-stream-sha256'] == digest(V / 'native-base-final-build-user-artifacts.jsonl'),
     'Revised churn Cargo artifact provenance')
new_archive = load(V / 'namespace-churn-new-archive-after.json')
need(new_archive['Path'] in churn['cargo-artifact']['filenames'], 'Revised churn selected core archive')
old_archive = load(D / 'development/runtime-epoch1/namespace-churn-old-archive-after.json')
need(new_archive['Hash'] != old_archive['Hash'], 'Revised churn rebuilt archive')
text = (V / 'namespace-churn-new.log').read_text(encoding='utf-8-sig')
rows = re.findall(r'^capture index=(\d+) result=(\w+)$', text, re.M)
need(rows == [(str(i), 'ok') for i in range(16)], 'Revised churn successful captures')
result = re.search(r'^complete captures=16 ok=16 failed=0 created=(\d+) closed=(\d+) '
                   r'error=None scope=capture-only-no-guest-execution$', text, re.M)
need(result is not None and result[1] == result[2] and int(result[1]) > 0,
     'Revised churn own event handle lifetime')

print('Base directory validation: final source, gates, carriers and ordinary boundary verified')
