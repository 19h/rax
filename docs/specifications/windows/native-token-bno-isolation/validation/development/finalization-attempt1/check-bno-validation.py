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

manifest = load(V / 'bno-epoch5-source.json')
need(manifest['base'] == 'be05c4b474dbf76be910e0f50dbc4be3262c16ce' and
     manifest['source-epoch'] == 5 and manifest['namespace-epoch'] == 2, 'tested source lineage')
inputs = manifest['files']
need(len(inputs) == len({r['path'] for r in inputs}) == 5737, 'selected source inputs')
for row in inputs:
    source(row)
port = load(V / 'final-port.json')
parent = load(V / 'base-final-main-state.json')
need(port['parent'] == parent['rax'] and port['compiled-inputs'] == 5737,
     'exact port onto published namespace parent')
completion = load(V / 'bno-final-post-run-completion.json')
need(completion['source-epoch'] == 5 and completion['namespace-epoch'] == 2 and
     [(r['script'], r['exit-code']) for r in completion['checks']] ==
     [('run-bno-owner-observer.py', 0), ('verify-bno-final-source.py', 0),
      ('verify-bno-post-run.py', 0)], 'native completion checks')
need('Verified final native BNO source inputs: 5737' in
     (V / 'native-bno-final-source-post-run.log').read_text(), 'native final source check')
compared = load(V / 'current-cpp-consumers-bno-root.json')
need(compared['current-root-head'] == parent['root'] and len(compared['files']) == 9,
     'current C++ source comparison')
before = load(V / 'native-bno-epoch5-archive-before.json')
after = load(V / 'native-bno-epoch5-archive-after.json')
need(before == after and before['Hash'].lower() ==
     '3384892ff5a48b7ad94c98088e4c5ebc5aace0d7ff5d1f55588b3490a8c2e3ca',
     'unchanged selected ordinary core archive')
messages = [json.loads(line) for line in
            (V / 'native-bno-epoch5-build-user-artifacts.jsonl').read_text().splitlines()]
artifacts = [row for row in messages if row.get('reason') == 'compiler-artifact'
             and row['target']['name'] == 'rax' and before['Path'] in row['filenames']]
need(len(artifacts) == 1 and artifacts[0]['features'] == [],
     'ordinary observer actual empty-feature Cargo core')
launch = load(V / 'bno-native-epoch5-launch.json')
need(launch['post-source-inputs'] == 5737 and launch['source-epoch'] == 5
     and launch['namespace-epoch'] == 2, 'complete native post-run source identity')

post_run = {r['path'].replace('\\', '/').split('/')[-1]: r
            for r in load(V / 'native-bno-final-post-run-identities.json')}
for name, expected in {
    'native-shared-section-trace.rs': '514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0',
    'config.toml': '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c',
    'rax_process.cpp': 'ed55d78681a9a59e384186382d998244d80ca5665793e60f5e7577d88d526564',
    'ntdll.dll': '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'whoami.exe': '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6',
    'smoke.exe': '325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
}.items():
    need(post_run[name]['sha256'] == expected, 'post-run identity ' + name)
need(post_run['native-bno-epoch5-trace.exe']['bytes'] > 0,
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
for host, total, failures in [('macos', 7929, 0), ('linux', 7923, 0), ('native', 7367, 5)]:
    name = host + '-bno-epoch5-full.log'
    text = (V / name).read_text(encoding='utf-8-sig')
    need(counts(name) == [(total, failures, 2, 0)], 'full test counts ' + host)
    need(set(re.findall(r'^test (\S+) \.\.\. ignored$', text, re.M)) == ignored,
         'ignored test identities ' + host)
    if failures:
        failed = set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))
        need(failed == baseline_failures, 'native baseline failure identities')
    gates = load(V / (host + '-bno-epoch5-gates.json'))
    need([r['gate'] for r in gates] == (
        ['focus', 'integration', 'capi', 'full', 'all-targets'] if host == 'native'
        else ['full', 'capi', 'integration', 'all-targets']), 'gate selection ' + host)
    for row in gates:
        command = ' '.join(row['argv']) if 'argv' in row else row['command']
        need(all(flag in command for flag in ['--locked', '--offline', '--no-default-features']),
             'core configuration ' + host + '/' + row['gate'])
        selection = {'focus': ['--lib', 'bno_'], 'full': ['--lib'], 'capi': ['-p rax-capi'],
                     'integration': ['--test user_windows', '--test user_windows_memory'],
                     'all-targets': ['check', '--all-targets']}[row['gate']]
        need(all(flag in command for flag in selection), 'core target selection')
        expected = 101 if host == 'native' and row['gate'] == 'full' else 0
        need(row['exit-code'] == expected, 'gate exit ' + host + '/' + row['gate'])
        if 'sha256' in row:
            need(digest(V / row['log']) == row['sha256'], 'gate log identity')
    need((168, 0, 0, 0) in counts(host + '-bno-epoch5-capi.log'), 'C API count ' + host)
    expected = [(0, 0, 0, 0), (4, 0, 0, 0)] if host == 'native' else [
        (544, 0, 0, 0), (0, 0, 0, 0)]
    need(counts(host + '-bno-epoch5-integration.log') == expected,
         'integration selection ' + host)

for host, log, passed, filtered in [
    ('macos', 'prototype-macos-focus-epoch5.log', 8, 7923),
    ('linux', 'prototype-linux-focus-epoch5.log', 8, 7917),
    ('native', 'native-bno-epoch5-focus.log', 9, 7365),
]:
    need(counts(log) == [(passed, 0, 0, filtered)], 'focused BNO result ' + host)
for name in ['bno-native-epoch5-launch.json', 'bno-final-owned-launch.json']:
    row = load(V / name)
    need(row['state'] == 'finished' and row['exit-code'] == 0, 'native driver completion')
for host, expected in [('macos', 7), ('linux', 4)]:
    gates = load(V / (host + '-bno-owning-gates.json'))
    need(len(gates) == expected, 'owning gate count ' + host)
    for row in gates:
        need(row['exit-code'] == 0 and digest(V / row['log']) == row['sha256'],
             'owning gate and log ' + host)

for host in ['macos', 'linux', 'native']:
    archive = load(V / (host + '-bno-final-owning-archive-identities.json'))
    need(archive['source-epoch'] == 'native-bno-isolation-epoch5', 'owning source epoch')
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
    need(len(archive['archives']) == 2 and len(archive['sources']) == 428,
         'owning archives and selected sources ' + host)
    for row in archive['sources']:
        source(row)
    for row in archive['archives']:
        need(row['structural-member-walk'] == 'pass' and row['members'] > 0
             and row['bytes'] > 0 and len(row['sha256']) == 64, 'complete archive walk')
    filenames = archive['core-artifact']['filenames'] + archive['carrier-artifact']['filenames']
    need(all(row['path'] in filenames for row in archive['archives']), 'actual Cargo artifacts')

ctest = (V / 'macos-bno-owning-ctest.log').read_text()
passed = re.findall(r'^\d+/7 Test #\d+: (\S+)\s+\.+\s+Passed', ctest, re.M)
need(len(passed) == 7 and set(passed) == {
    'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
    'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
    'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test',
}, 'macOS seven CTest outcomes')
for host, name, process_checks in [('native', 'assist-native-bno-final-cpp.log', 182),
                                    ('linux', 'linux-bno-owning-cpp.log', 184)]:
    text = (V / name).read_text(encoding='utf-8-sig')
    for token in [str(process_checks) + ' checks passed', '162 checks passed', '2/2 passed',
                  'PASS', 'canonical discovery and explicit RAX-disabled refusal passed']:
        need(token in text, 'C++ consumer ' + host + '/' + token)

ordinary = load(V / 'ordinary-analysis.json')
review = load(V / 'bno-ordinary-reviewed.json')
contract = review['outcomes']
executed = [r for r in ordinary if 'log' in r]
need(review['status'] == 'reviewed' and review['source-epoch'] == 5 and
     len(executed) == len(contract) == 2, 'ordinary execution count')
for row, expected in zip(executed, contract):
    need(row['controller-exit-code'] == 0 and row['program-sha256'] ==
         row['program-sha256-after'] == expected['program-sha256'], 'ordinary target identity')
    need(row['sha256'] == digest(V / row['log']) == expected['sha256'], 'ordinary log identity')
    text = (V / row['log']).read_text(encoding='utf-8-sig')
    need([line for line in text.splitlines() if line.startswith('terminal ')] ==
         [expected['terminal']], 'actual ordinary terminal')
    turn = expected['class44-success-turn']
    need(f'kernel-after turn={turn} service=0x21 PC=0x7ff82fad1224 X0=0x0' in text,
         'class44 now returns success')
    need(expected['terminal-turn'] - turn == expected['observer-turns-advanced'] == 100,
         'bounded observer progression')
    need('NtCreateMutant (0xba)' in expected['terminal'] and
         'is not implemented' in expected['terminal'], 'no invented guest exit')
owner = load(V / 'bno-owner-observer-launch.json')
need(owner['state'] == 'finished' and owner['exit_code'] == 0, 'owner diagnostic completed')
for label in ['source', 'executable', 'archive']:
    before = owner[label + '-before'] if label != 'archive' else owner['archive']
    after = owner[label + '-after']
    need(before['sha256'] == after['sha256'] and before['bytes'] == after['bytes'],
         'owner diagnostic before/after identity ' + label)
need(owner['source-before']['sha256'] ==
     '1a5d047c324cdcc365d697273bacca64536d0c4366ee58dc0e66764119efa907',
     'unchanged fast diagnostic source')
need(digest(V / 'owner-diagnostic/bno-owner-observer.rs') == owner['source-before']['sha256'],
     'retained diagnostic source bytes')
archives = load(V / 'native-bno-final-owning-archive-identities.json')['archives']
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


print('BNO final source, native/copy proof, three-host gates and retained consumers verified')
