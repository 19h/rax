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

manifest = load(V / 'scalar-final-source.json')
need(manifest['base'] == '19f28635c84a62dbc99b67d4775e8289dd6bc5ba', 'source parent')
inputs = manifest['files']
need(len(inputs) == len({r['path'] for r in inputs}) == 5718, 'selected final source inputs')
for row in inputs:
    source(row)

upstream = load(V / 'upstream-cpp-consumer-compatibility.json')
compared = load(V / 'current-cpp-consumers-scalar-root.json')
need(upstream['files'] == compared['files'] and len(upstream['files']) == 9,
     'preserved upstream C++ source identities')
need(upstream['compared-root'] == compared['current-root-head'] and
     upstream['frozen-carrier-abi-minor'] == 184 and upstream['upstream-abi-minor'] >= 200,
     'retained carrier versus upstream distinction')

completion = load(V / 'scalar-final-post-run-completion.json')
need([(r['script'], r['exit-code']) for r in completion['checks']] ==
     [('verify-scalar-final.py', 0), ('verify-scalar-post-run.py', 0)],
     'post-completion native source and artifact verification')
need('Verified final native token source inputs: 5718' in
     (V / 'native-scalar-final-source-post-run.log').read_text(encoding='utf-8-sig'),
     'post-completion complete selected source check')

post_run = {r['path'].replace('\\', '/').split('/')[-1]: r
            for r in load(V / 'native-scalar-final-post-run-identities.json')}
for name, expected in {
    'native-shared-section-trace.rs': '514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0',
    'config.toml': '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c',
    'rax_process.cpp': 'ed55d78681a9a59e384186382d998244d80ca5665793e60f5e7577d88d526564',
    'ntdll.dll': '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'whoami.exe': '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6',
    'smoke.exe': '325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
}.items():
    need(post_run[name]['sha256'] == expected, 'post-run identity ' + name)
need(post_run['native-scalar-final-trace.exe']['bytes'] > 0,
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
for host, total, failures in [('macos', 7909, 0), ('linux', 7903, 0), ('native', 7345, 5)]:
    name = host + '-scalar-final-full.log'
    text = (V / name).read_text(encoding='utf-8-sig')
    need(counts(name) == [(total, failures, 2, 0)], 'full test counts ' + host)
    need(set(re.findall(r'^test (\S+) \.\.\. ignored$', text, re.M)) == ignored,
         'ignored test identities ' + host)
    if failures:
        failed = set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))
        need(failed == baseline_failures, 'native baseline failure identities')
    gates = load(V / (host + '-scalar-final-gates.json'))
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
    need((168, 0, 0, 0) in counts(host + '-scalar-final-capi.log'), 'C API count ' + host)
    expected = [(0, 0, 0, 0), (4, 0, 0, 0)] if host == 'native' else [
        (544, 0, 0, 0), (0, 0, 0, 0)]
    need(counts(host + '-scalar-final-integration.log') == expected,
         'integration selection ' + host)

for epoch in [1, 2]:
    for host, passed, filtered in [('macos', 29, 7882), ('linux', 29, 7876), ('native', 34, 7318)]:
        need(counts(f'{host}-scalar-epoch{epoch}-focus.log') == [(passed, 0, 0, filtered)],
             'scalar focused result ' + host + '/' + str(epoch))
initial = load(V / 'scalar-epoch1-source.json')
final = load(V / 'scalar-epoch2-source.json')
old = {r['path']: r for r in initial['files']}
changed = {r['path'] for r in final['files'] if old.get(r['path']) != r}
need(changed == {'src/user/windows/wnf/installed/scalars.rs',
                 'src/user/windows/process/sched/tests/services_tests/token_scalar_tests.rs'},
     'explicit alignment correction source boundary')
for name in ['scalar-epoch1-source.json', 'scalar-epoch2-source.json']:
    need(len(load(V / name)['files']) == 5718, 'scalar epoch source count')

for name in ['scalar-final-launch.json', 'scalar-final-owned-launch.json']:
    row = load(V / name)
    need(row['state'] == 'finished' and row['exit-code'] == 0, 'native driver completion')

for host, expected in [('macos', 7), ('linux', 4)]:
    gates = load(V / (host + '-scalar-owning-gates.json'))
    need(len(gates) == expected, 'owning gate count ' + host)
    for row in gates:
        need(row['exit-code'] == 0 and digest(V / row['log']) == row['sha256'],
             'owning gate and log ' + host)

for host in ['macos', 'linux', 'native']:
    archive = load(V / (host + '-scalar-final-owning-archive-identities.json'))
    need(archive['source-epoch'] == 'native-token-scalars-final', 'owning source epoch')
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
    need(len(archive['archives']) == 2 and len(archive['sources']) == 409,
         'owning archives and selected sources ' + host)
    for row in archive['sources']:
        source(row)
    for row in archive['archives']:
        need(row['structural-member-walk'] == 'pass' and row['members'] > 0
             and row['bytes'] > 0 and len(row['sha256']) == 64, 'complete archive walk')
    filenames = archive['core-artifact']['filenames'] + archive['carrier-artifact']['filenames']
    need(all(row['path'] in filenames for row in archive['archives']), 'actual Cargo artifacts')

ctest = (V / 'macos-scalar-owning-ctest.log').read_text()
passed = re.findall(r'^\d+/7 Test #\d+: (\S+)\s+\.+\s+Passed', ctest, re.M)
need(len(passed) == 7 and set(passed) == {
    'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
    'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
    'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test',
}, 'macOS seven CTest outcomes')
for host, name, process_checks in [('native', 'assist-native-scalar-final-cpp.log', 182),
                                    ('linux', 'linux-scalar-owning-cpp.log', 184)]:
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
    frontier_turn = [2149956, 2152305][index]
    expected_terminal = ['terminal turn=2149956 Complete(Internal("native NTDLL service NtOpenDirectoryObject: Unimplemented(\\"NtOpenDirectoryObject: NT directory outside selected installed namespace\\")"))', 'terminal turn=2152305 Complete(Internal("native NTDLL service NtOpenDirectoryObject: Unimplemented(\\"NtOpenDirectoryObject: NT directory outside selected installed namespace\\")"))'][index]
    need(row['terminal-lines'][0] == expected_terminal and
         f'kernel-before turn={frontier_turn} service=0x58' in text,
         'recorded uncaptured NT-directory boundary; no successful guest exit')

print('Scalar validation: final source, gates, carriers and ordinary boundary verified')
