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

manifest = load(V / 'session-final-source.json')
need(manifest['base'] == 'c68d2c589a7121a3dcd9263fbbfa5854238b7d0b', 'source parent')
inputs = manifest['files']
need(len(inputs) == len({r['path'] for r in inputs}) == 5713, 'selected final source inputs')
for row in inputs:
    source(row)

upstream = load(V / 'upstream-cpp-consumer-compatibility.json')
compared = load(V / 'current-cpp-consumers-session-root.json')
need(upstream['files'] == compared['files'] and len(upstream['files']) == 9,
     'preserved upstream C++ source identities')
need(upstream['compared-root'] == compared['current-root-head'] and
     upstream['frozen-carrier-abi-minor'] == 184 and upstream['upstream-abi-minor'] >= 200,
     'retained carrier versus upstream distinction')

post_run = {r['path'].replace('\\', '/').split('/')[-1]: r
            for r in load(V / 'native-session-final-post-run-identities.json')}
for name, expected in {
    'native-shared-section-trace.rs': '514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0',
    'config.toml': '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c',
    'rax_process.cpp': 'ed55d78681a9a59e384186382d998244d80ca5665793e60f5e7577d88d526564',
    'ntdll.dll': '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'whoami.exe': '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6',
    'smoke.exe': '325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
}.items():
    need(post_run[name]['sha256'] == expected, 'post-run identity ' + name)
need(post_run['native-session-final-trace.exe']['bytes'] > 0,
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
for host, total, failures in [('macos', 7906, 0), ('linux', 7900, 0), ('native', 7341, 5)]:
    name = host + '-session-final-full.log'
    text = (V / name).read_text(encoding='utf-8-sig')
    need(counts(name) == [(total, failures, 2, 0)], 'full test counts ' + host)
    need(set(re.findall(r'^test (\S+) \.\.\. ignored$', text, re.M)) == ignored,
         'ignored test identities ' + host)
    if failures:
        failed = set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))
        need(failed == baseline_failures, 'native baseline failure identities')
    gates = load(V / (host + '-session-final-gates.json'))
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
    need((168, 0, 0, 0) in counts(host + '-session-final-capi.log'), 'C API count ' + host)
    expected = [(0, 0, 0, 0), (4, 0, 0, 0)] if host == 'native' else [
        (544, 0, 0, 0), (0, 0, 0, 0)]
    need(counts(host + '-session-final-integration.log') == expected,
         'integration selection ' + host)

need(counts('macos-session-id-epoch2-focus.log') == [(26, 0, 0, 7882)], 'macOS focus')
need(counts('linux-session-id-epoch2-focus.log') == [(26, 0, 0, 7876)], 'Linux focus')
need(counts('native-session-id-epoch2-complete-focus.log') == [(30, 0, 0, 7318)],
     'native focus and installed leaf')
for host in ['macos', 'linux']:
    initial = (V / 'epoch1' / (host + '-session-id-epoch1-focus.log')).read_text()
    need('error[E0308]' in initial and 'Arc<Subject>' in initial,
         'retained initial fixture compilation failure ' + host)
initial = (V / 'epoch1/native-session-id-epoch1-focus.log').read_text(encoding='utf-8-sig')
need('capi' in initial and 'Cargo.toml' in initial and 'The system cannot find the file specified' in initial,
     'retained native workspace manifest failure')
initial = (V / 'preparation/native-session-id-epoch2-focus.log').read_text(encoding='utf-8-sig')
need('linux-loader' in initial and 'requires a lock file to be present first' in initial,
     'retained native workspace lockfile failure')
for name, expected in [('session-id-epoch1-source.json', 5680),
                       ('session-id-epoch2-source.json', 5712),
                       ('session-id-epoch2-complete-source.json', 5713)]:
    need(len(load(V / 'preparation' / name)['files']) == expected,
         'preparation source count ' + name)

for name in ['session-final-launch.json', 'session-final-owned-launch.json']:
    row = load(V / name)
    need(row['state'] == 'finished' and row['exit-code'] == 0, 'native driver completion')

for host, expected in [('macos', 7), ('linux', 4)]:
    gates = load(V / (host + '-session-owning-gates.json'))
    need(len(gates) == expected, 'owning gate count ' + host)
    for row in gates:
        need(row['exit-code'] == 0 and digest(V / row['log']) == row['sha256'],
             'owning gate and log ' + host)

for host in ['macos', 'linux', 'native']:
    archive = load(V / (host + '-session-final-owning-archive-identities.json'))
    need(archive['source-epoch'] == 'native-token-session-id-final', 'owning source epoch')
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
    need(len(archive['archives']) == 2 and len(archive['sources']) == 404,
         'owning archives and selected sources ' + host)
    for row in archive['sources']:
        source(row)
    for row in archive['archives']:
        need(row['structural-member-walk'] == 'pass' and row['members'] > 0
             and row['bytes'] > 0 and len(row['sha256']) == 64, 'complete archive walk')
    filenames = archive['core-artifact']['filenames'] + archive['carrier-artifact']['filenames']
    need(all(row['path'] in filenames for row in archive['archives']), 'actual Cargo artifacts')

ctest = (V / 'macos-session-owning-ctest.log').read_text()
passed = re.findall(r'^\d+/7 Test #\d+: (\S+)\s+\.+\s+Passed', ctest, re.M)
need(len(passed) == 7 and set(passed) == {
    'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
    'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
    'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test',
}, 'macOS seven CTest outcomes')
for host, name, process_checks in [('native', 'assist-native-session-final-cpp.log', 182),
                                    ('linux', 'linux-session-owning-cpp.log', 184)]:
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
    need('Complete(Internal(' in expected['terminal'], 'ordinary completion remains unproven')
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
    pinned_terminals = ['terminal turn=2149815 Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\\"NtQueryInformationToken: NtQueryInformationToken class 42\\")"))', 'terminal turn=2152164 Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\\"NtQueryInformationToken: NtQueryInformationToken class 42\\")"))']
    need(expected['terminal'] == pinned_terminals[index] and
         f'kernel-before turn={query_turn + 27} service=0x21 PC=0x7ff82fad1220 registers=[6c, 2a,' in text,
         'exact next class42 boundary')

print('SessionId validation: final source, gates, carriers and ordinary boundary verified')
