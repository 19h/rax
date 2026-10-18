"""Verify source epochs, real gate selections, retained consumers and startup."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re

D = Path(__file__).resolve().parent
V = D / 'validation'
parser = argparse.ArgumentParser()
parser.add_argument('--source-root', type=Path, default=D.parents[3])
P = parser.parse_args().source_root


def need(value, message):
    if not value:
        raise ValueError(message)


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else raw.decode('utf-8-sig'))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source(row):
    path = (P / row['path']).resolve()
    need(path.is_relative_to(P.resolve()), 'source path outside repository')
    raw = path.read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], 'source identity ' + row['path'])


def counts(name):
    return [tuple(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',
        (V / name).read_text(encoding='utf-8-sig'))]


need(__debug__, 'optimized checker rejected')
need((D / '.gitattributes').read_text() == '* -text\n', 'raw proof line endings')
for row in load(D / 'line-ending-verification.json')['files']:
    raw = (D / row['path']).read_bytes()
    blob = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
    need(row['raw-git-blob'] == row['autocrlf-true-git-blob'] == blob, 'raw line-ending identity')
seen = set()
for row in load(V / 'inputs.json')['files']:
    path = (D / row['path']).resolve()
    need(path.is_relative_to(D) and row['path'] not in seen, 'duplicate or escaped evidence path')
    seen.add(row['path'])
    raw = path.read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], 'evidence identity ' + row['path'])

manifest = load(V / 'mutant-source-epoch4.json')
need(manifest['source-epoch'] == 4 and manifest['base'] == '390309d0a09f58643c38e7014adea82742bd2c41', 'final source ancestry')
need(len(manifest['files']) == len({r['path'] for r in manifest['files']}) == 5762, 'selected source and fixture inputs')
for row in manifest['files']:
    source(row)
restored_review = load(V / 'mutant-native-restored-inputs-review.json')
exceptions = {row['path']: row for row in restored_review['representation-or-document-differences']}
documents = {path for path, row in exceptions.items() if row['classification'].startswith('Retained')}
expected_documents = {'src/user/windows/native-runtime.md'} | {
    'docs/specifications/windows/' + group + '/' + name
    for group in ['native-process-token-open', 'native-system-time']
    for name in ['assumption-register.json', 'bounded-scope.json', 'check-native.py', 'README.md']}
need(documents == expected_documents and len(exceptions) == 25, 'bounded historical and symlink input differences')
need(not documents.intersection(row['path'] for row in manifest['files']), 'historical docs do not replace selected compiled inputs')
need(digest(V / 'mutant-native-cargo-config.toml') == '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c', 'native source-only Cargo configuration')

ignored = {'smir::lower::validation::tests::test_lift_lower_full_microkernel',
           'smir::lower::validation::tests::test_roundtrip_exact_bytes_microkernel'}
baseline_failures = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}
for host, stem, passed, failed, filtered in [
    ('macos', 'mutant-macos-epoch4', 7945, 0, 7936),
    ('linux', 'mutant-linux-epoch4', 7939, 0, 7930),
    ('native', 'mutant-native-epoch4c', 7384, 5, 7380),
]:
    state = load(V / (stem + '-gates.json'))
    need(state['state'] == 'finished' and state['exit-code'] == 0, 'finished driver ' + host)
    steps = state['steps']
    expected = ['full', 'capi', 'integration', 'all-targets'] if host == 'macos' else ['focus', 'full', 'capi', 'integration', 'all-targets']
    if host == 'native':
        expected.insert(0, 'toolchain')
    need([r['name'] for r in steps] == expected, 'complete gate selection ' + host)
    for row in steps:
        need(digest(V / row['log']) == row['sha256'], 'gate log identity ' + host + '/' + row['name'])
        if row['name'] == 'toolchain':
            need(row['exit-code'] == 0 and 'cargo 1.95.0' in (V / row['log']).read_text(), 'native compiler version')
            continue
        command = ' '.join(row['argv'])
        need(all(flag in command for flag in ['--locked', '--offline', '--no-default-features']), 'portable core feature policy')
        flags = {'focus': ['--lib native_mutant'], 'full': ['--lib'], 'capi': ['-p rax-capi'],
                 'integration': ['--test user_windows', '--test user_windows_memory'],
                 'all-targets': ['check', '--all-targets']}[row['name']]
        need(all(flag in command for flag in flags), 'target selection ' + host)
        need(row['exit-code'] == (101 if host == 'native' and row['name'] == 'full' else 0), 'gate status ' + host)
    full = (V / (stem + '-full.log')).read_text(encoding='utf-8-sig')
    need(counts(stem + '-full.log') == [(passed, failed, 2, 0)], 'full unfiltered counts ' + host)
    need(set(re.findall(r'^test (\S+) \.\.\. ignored\r?$', full, re.M)) == ignored, 'ignored identities ' + host)
    if failed:
        tail = full[full.rfind('\nfailures:\n'):]
        need(set(re.findall(r'^    (\S+)\r?$', tail, re.M)) == baseline_failures, 'exact five existing Windows failures')
        need(set(state['exact-existing-failures']) == baseline_failures, 'native baseline reconciliation')
        need(state['prepared-source-inputs'] == state['post-source-inputs'] == 5762, 'native source before/after')
        need(len(state['supplemental-package-files']) == 23 and len(state['restored-baseline-files']) == 13236,
             'repaired native compilation input closure')
        for row in state['supplemental-package-files'] + state['restored-baseline-files']:
            exception = exceptions.get(row['path'])
            if exception is None:
                source(row)
            else:
                need(exception['expected'] == row, 'explicit restored-input identity')
                if row['path'] in documents:
                    raw = (V / 'native-restored-documents' / row['path']).read_bytes()
                else:
                    path = P / row['path']
                    raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
                need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], 'restored document or symlink text')
    need((168, 0, 0, 0) in counts(stem + '-capi.log'), 'C API selection ' + host)
    integration = [(0, 0, 0, 0), (4, 0, 0, 0)] if host == 'native' else [(544, 0, 0, 0), (0, 0, 0, 0)]
    need(counts(stem + '-integration.log') == integration, 'native versus Unix integration selection ' + host)
    focus = 'mutant-focus-epoch4.log' if host == 'macos' else stem + '-focus.log'
    need(counts(focus) == [(11, 0, 0, filtered)], 'all eleven focused tests ' + host)

need(counts('mutant-baseline-epoch4.log') == [(0, 11, 0, 7936)], 'tests-only published-parent failure baseline')
baseline_text = (V / 'mutant-baseline-epoch4.log').read_text()
baseline_ids = set(re.findall(r'^    (\S+)\r?$', baseline_text[baseline_text.rfind('\nfailures:\n'):], re.M))
need(len(baseline_ids) == 11, 'eleven exact baseline regression identities')
for name in ['mutant-focus-epoch4.log', 'mutant-linux-epoch4-focus.log', 'mutant-native-epoch4c-focus.log']:
    passed_ids = set(re.findall(r'^test (\S+) \.\.\. ok\r?$', (V / name).read_text(), re.M))
    need(passed_ids == baseline_ids, 'same focused regressions on every host')
for host, total in [('macos', 7), ('linux', 4)]:
    gates = load(V / (host + '-mutant-owning-gates.json'))
    need(len(gates) == total and all(row['exit-code'] == 0 for row in gates), 'retained owner gates ' + host)
    for row in gates:
        need(digest(V / row['log']) == row['sha256'], 'retained owner log ' + host)
need(load(V / 'mutant-final-owned-launch.json')['state'] == 'finished', 'native retained owner completion')
for host in ['macos', 'linux', 'native']:
    archive = load(V / (host + '-mutant-final-owning-archive-identities.json'))
    need(archive['source-epoch'] == 'native-mutant-epoch4' and archive['core-features'] == [], 'empty-feature current archive ' + host)
    metadata = archive['compiler-messages']
    name = metadata['path'].replace('\\', '/').split('/')[-1]
    raw = (V / name).read_bytes()
    need(len(raw) == metadata['bytes'] and hashlib.sha256(raw).hexdigest() == metadata['sha256'], 'actual Cargo message stream')
    messages = []
    for line in raw.decode('utf-8-sig').splitlines():
        try:
            messages.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    need(archive['core-artifact'] in messages and archive['carrier-artifact'] in messages, 'actual selected artifacts ' + host)
    need(archive['core-artifact']['features'] == [], 'actual core features ' + host)
    need(len(archive['archives']) == 2 and len(archive['sources']) == 456, 'two actual archives and selected owner source ' + host)
    for row in archive['sources']:
        source(row)
    filenames = archive['core-artifact']['filenames'] + archive['carrier-artifact']['filenames']
    for row in archive['archives']:
        need(row['path'] in filenames and row['structural-member-walk'] == 'pass' and row['members'] > 0 and row['bytes'] > 0,
             'complete selected archive walk ' + host)

ctest = (V / 'macos-mutant-owning-ctest.log').read_text()
passed_ctests = re.findall(r'^\d+/7 Test #\d+: (\S+)\s+\.+\s+Passed', ctest, re.M)
need(len(passed_ctests) == 7 and set(passed_ctests) == {
    'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
    'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
    'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test'}, 'seven exact macOS CTest outcomes')
for host, name, process_count in [('native', 'assist-native-mutant-final-cpp.log', 182), ('linux', 'linux-mutant-owning-cpp.log', 184)]:
    text = (V / name).read_text(encoding='utf-8-sig')
    for token in [str(process_count) + ' checks passed', '162 checks passed', '2/2 passed', 'PASS',
                  'canonical discovery and explicit RAX-disabled refusal passed']:
        need(token in text, 'C++ consumer ' + host + '/' + token)

comparison = load(V / 'mutant-upstream-source-comparison.json')
need(comparison['frozen-carrier-abi-minor'] == 184 and comparison['matched-affected-files'] == 8, 'frozen/current root separation')
need(len(comparison['files']) == 9 and sum(row['equal'] for row in comparison['files']) == 8, 'current C++ consumers')
for row in comparison['files']:
    need(row['equal'] == (row['compared'] == row['upstream']), 'comparison classification')
    if not row['equal']:
        need(row['upstream']['path'] == 'src/crypto/static_text.cpp' and row['upstream']['sha256'] ==
             '1835211daeff4b7089081214c070f78f73606ec0f9a1f99cf448280402580372', 'separate static-text migration')

owner = load(V / 'mutant-owner-observer-launch.json')
need(owner['state'] == 'finished' and owner['exit_code'] == 0 and len(owner['outcomes']) == 2, 'two ordinary owner observations')
for label in ['source', 'executable', 'archive']:
    before = owner['archive'] if label == 'archive' else owner[label + '-before']
    after = owner[label + '-after']
    need(before['sha256'] == after['sha256'] and before['bytes'] == after['bytes'], 'observer unchanged ' + label)
need(owner['source-before']['sha256'] == 'adff6a95e16d29c0559585226c67fa5178f01e617404ebb84de583d698ea56df', 'observer source identity')
need(digest(V / 'owner-diagnostic/mutant-owner-observer.rs') == owner['source-before']['sha256'], 'retained observer source')
archives = load(V / 'native-mutant-final-owning-archive-identities.json')['archives']
need(any(row['sha256'] == owner['archive']['sha256'] and row['path'] == owner['archive']['path'] for row in archives), 'observer linked actual retained core')
programs = ['325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
            '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6']
expected = load(V / 'mutant-ordinary-reviewed.json')
need(expected['successful-guest-exits'] == 0, 'full goal remains incomplete')
for index, row in enumerate(owner['outcomes']):
    need(row['exit-code'] == 0 and row['program-before'] == row['program-after'] and row['program-before']['sha256'] == programs[index], 'unchanged ordinary target')
    need(digest(V / row['log']) == row['log-identity']['sha256'], 'ordinary log identity')
    text = (V / row['log']).read_text()
    need('prefix-turns=2100000 result=BudgetExhausted' in text, 'actual unchanged startup prefix')
    need('Complete(Internal("native NTDLL service NtWaitForSingleObject (0x4)' in expected['programs'][index]['terminal-line'], 'explicit remaining native wait boundary')
    need('status=Some(0)' in expected['programs'][index]['mutant-result-line'] and 'kind=Some("Mutant")' in expected['programs'][index]['mutant-result-line'], 'successful private mutant result')
    need(expected['programs'][index]['terminal-line'] in text, 'ordinary terminal evidence')
    need(expected['programs'][index]['mutant-result-line'] in text, 'ordinary mutant result evidence')

print('Mutant source epochs, three-host gates, retained consumers and ordinary startup verified')
