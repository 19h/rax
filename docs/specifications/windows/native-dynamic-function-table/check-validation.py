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
seen = set()
for row in load(V / 'inputs.json')['files']:
    need(row['path'] not in seen, 'duplicate evidence path')
    seen.add(row['path'])
    raw = (D / row['path']).read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
         'evidence identity ' + row['path'])

inputs = load(V / 'dynamic-final-source.json')['files']
need(len(inputs) == len({r['path'] for r in inputs}) == 5566, 'selected final source inputs')
for row in inputs:
    source(row)

post_run = {r['path'].replace('\\', '/').split('/')[-1]: r
            for r in load(V / 'native-dynamic-final-post-run-identities.json')}
for name, expected in {
    'native-shared-section-trace.rs': '514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0',
    'config.toml': '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c',
    'rax_process.cpp': 'ed55d78681a9a59e384186382d998244d80ca5665793e60f5e7577d88d526564',
    'ntdll.dll': '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'whoami.exe': '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6',
    'smoke.exe': '325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
}.items():
    need(post_run[name]['sha256'] == expected, 'post-run identity ' + name)
need(post_run['native-dynamic-final-trace.exe']['bytes'] > 0,
     'post-run ordinary observer executable')

for name, expected in {
    'dynamic-epoch1-memory-wrapper.reconstructed.rs.txt': 'deccf5fd7ebb69a63ab7eaff82695d0b5fb36dc139fed57cd6eb3c142ca5c480',
    'dynamic-epoch2-memory-wrapper.rs.txt': 'c80b9faca822f7cdbea953a5a3b58e737444c61ca41b9c555828ceab67508b57',
    'dynamic-epoch3-memory-wrapper.rs.txt': 'fe065d11866b942992f9e9e3809994a61fd41d60507557f5172d1ca141679277',
}.items():
    need(digest(V / 'failed-epochs' / name) == expected, 'retained source epoch ' + name)
for host in ['macos', 'linux']:
    old = (V / 'failed-epochs' / (host + '-dynamic-epoch1-focus.log')).read_text()
    need(old.count('error[E0277]') == 2, 'retained wrong-error-type build ' + host)
    name = 'failed-epochs/' + host + '-dynamic-epoch2-focus.log'
    need(counts(name)[0][:3] == (10, 1, 0), 'retained whole-prefix failure ' + host)
    old = (V / name).read_text()
    need('"axis":"1"' in old and '"offset":"4080"' in old,
         'native inaccessible-prefix counterexample ' + host)

ignored = {'smir::lower::validation::tests::test_lift_lower_full_microkernel',
           'smir::lower::validation::tests::test_roundtrip_exact_bytes_microkernel'}
baseline_failures = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}
for host, total, failures in [('macos', 7891, 0), ('linux', 7885, 0), ('native', 7324, 5)]:
    name = host + '-dynamic-final-full.log'
    text = (V / name).read_text(encoding='utf-8-sig')
    need(counts(name) == [(total, failures, 2, 0)], 'full test counts ' + host)
    need(set(re.findall(r'^test (\S+) \.\.\. ignored$', text, re.M)) == ignored,
         'ignored test identities ' + host)
    if failures:
        failed = set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))
        need(failed == baseline_failures, 'native baseline failure identities')
    gates = load(V / (host + '-dynamic-final-gates.json'))
    need([r['gate'] for r in gates] == (
        ['integration', 'capi', 'full', 'all-targets'] if host == 'native'
        else ['full', 'capi', 'integration', 'all-targets']), 'gate selection ' + host)
    for row in gates:
        expected = 101 if host == 'native' and row['gate'] == 'full' else 0
        need(row['exit-code'] == expected, 'gate exit ' + host + '/' + row['gate'])
        if 'sha256' in row:
            need(digest(V / row['log']) == row['sha256'], 'gate log identity')
    need((168, 0, 0, 0) in counts(host + '-dynamic-final-capi.log'), 'C API count ' + host)
    expected = [(0, 0, 0, 0), (4, 0, 0, 0)] if host == 'native' else [
        (544, 0, 0, 0), (0, 0, 0, 0)]
    need(counts(host + '-dynamic-final-integration.log') == expected,
         'integration selection ' + host)

need(counts('macos-dynamic-final-focus.log') == [(11, 0, 0, 7882)], 'macOS focus')
need(counts('linux-dynamic-final-focus.log') == [(11, 0, 0, 7876)], 'Linux focus')
need(counts('native-dynamic-epoch3-focus-focus.log') == [(12, 0, 0, 7319)], 'native focus and leaf')
for name in ['dynamic-final-launch.json', 'dynamic-final-owned-launch.json']:
    row = load(V / name)
    need(row['state'] == 'finished' and row['exit-code'] == 0, 'native driver completion')

for host, expected in [('macos', 7), ('linux', 4)]:
    gates = load(V / (host + '-dynamic-owning-gates.json'))
    need(len(gates) == expected, 'owning gate count ' + host)
    for row in gates:
        need(row['exit-code'] == 0 and digest(V / row['log']) == row['sha256'],
             'owning gate and log ' + host)

for host in ['macos', 'linux', 'native']:
    archive = load(V / (host + '-dynamic-final-owning-archive-identities.json'))
    need(archive['core-features'] == [] and archive['core-artifact']['features'] == [],
         'empty-feature core ' + host)
    need(len(archive['archives']) == 2 and len(archive['sources']) == 361,
         'owning archives and selected sources ' + host)
    for row in archive['sources']:
        source(row)
    for row in archive['archives']:
        need(row['structural-member-walk'] == 'pass' and row['members'] > 0
             and row['bytes'] > 0 and len(row['sha256']) == 64, 'complete archive walk')
    filenames = archive['core-artifact']['filenames'] + archive['carrier-artifact']['filenames']
    need(all(row['path'] in filenames for row in archive['archives']), 'actual Cargo artifacts')

ctest = (V / 'macos-dynamic-owning-ctest.log').read_text()
passed = re.findall(r'^\d+/7 Test #\d+: (\S+)\s+\.+\s+Passed', ctest, re.M)
need(len(passed) == 7 and set(passed) == {
    'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
    'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
    'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test',
}, 'macOS seven CTest outcomes')
for host, name, process_checks in [('native', 'assist-native-dynamic-final-cpp.log', 182),
                                    ('linux', 'linux-dynamic-owning-cpp.log', 184)]:
    text = (V / name).read_text(encoding='utf-8-sig')
    for token in [str(process_checks) + ' checks passed', '162 checks passed', '2/2 passed',
                  'PASS', 'canonical discovery and explicit RAX-disabled refusal passed']:
        need(token in text, 'C++ consumer ' + host + '/' + token)

ordinary = load(V / 'ordinary-analysis.json')
contract = load(V / 'ordinary-contract.json')
executed = [r for r in ordinary if 'terminal-lines' in r]
need(len(executed) == len(contract) == 2, 'ordinary execution count')
for row, expected in zip(executed, contract):
    need(row['controller-exit-code'] == 0 and row['program-sha256'] == expected['program-sha256'],
         'ordinary controller and target identity')
    need(len(row['terminal-lines']) == 1 and row['terminal-lines'][0] == expected['terminal'],
         'ordinary measured terminal outcome')
    need('Complete(Internal(' in expected['terminal'], 'ordinary completion remains unproven')
    need(digest(V / row['log']) == row['sha256'], 'ordinary log identity')
    text = (V / row['log']).read_text(encoding='utf-8-sig')
    need(row['terminal-lines'][0] in text and 'registers=[ffffffffffffffff, 35,' in text,
         'ordinary class53 execution and terminal trace')

print('Dynamic-table validation: final source, gates, carriers and ordinary boundary verified')
