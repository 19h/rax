"""Check source identity, actual full selections and retained consumer evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re

COUNTS = {'macos': (8008, 0, 2), 'linux': (8002, 0, 2), 'native': (7461, 5, 2)}
FAILURES = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}
NATIVE_CONFIG = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'


def load(path):
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in (b'\xff\xfe', b'\xfe\xff')
                      else raw.decode('utf-8-sig'))


def bound_log(root, row):
    raw = (root / row['log']).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == row['sha256']
    assert len(raw) == row['bytes']
    return raw.decode('utf-8', 'replace')


def summary(text, passed, failed, ignored):
    pattern = (rf'test result: (?:ok|FAILED)\. {passed} passed; {failed} failed; '
               rf'{ignored} ignored; 0 measured; 0 filtered out;')
    assert re.search(pattern, text), (passed, failed, ignored)


def check(source, evidence):
    validation = evidence / 'validation'
    envelope_path = evidence / 'evidence-manifest.json'
    if envelope_path.exists():
        envelope = load(envelope_path)
        indexed = set()
        for item in envelope['files']:
            name = item['path']
            assert name not in indexed and not Path(name).is_absolute() and '..' not in Path(name).parts
            indexed.add(name)
            raw = (evidence / name).read_bytes()
            assert len(raw) == item['bytes'] and hashlib.sha256(raw).hexdigest() == item['sha256'], name
        actual = {str(path.relative_to(evidence)).replace('\\', '/') for path in evidence.rglob('*')
                  if path.is_file() and path != envelope_path}
        assert indexed == actual
    primary = load(evidence / 'primary-retrieval.json')
    assert primary['url'] == 'https://learn.microsoft.com/en-us/windows/win32/winprog64/shared-registry-keys'
    assert hashlib.sha256((evidence / 'shared-registry-keys.html').read_bytes()).hexdigest() == primary['sha256']
    host_identity = load(validation / 'timer2-native-host-identity.json')
    assert host_identity['machine'] == 'ARM64' and host_identity['pointer-bytes'] == 8
    assert host_identity['windows-version']['platform-version'] == [10, 0, 29683]
    assert host_identity['runtime-modules'][0]['sha256'] == '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
    raw_identity_source = (validation / 'capture-timer2-host-identity.py').read_bytes()
    assert hashlib.sha256(raw_identity_source).hexdigest() == host_identity['source-sha256']
    manifest = load(validation / 'loader-candidate-epoch3.json')
    source_rows = manifest['files']
    rows = source_rows
    assert len(rows) == 23283 and sum(row['bytes'] for row in rows) == 1657974640
    assert len({row['path'] for row in rows}) == len(rows)
    assert manifest['base-repository-commit'] == '9db68d792b386355b87a24b9d10466401ca71731'
    for row in rows:
        path = source / row['path']
        raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        if row['path'] == '.cargo/config.toml' and digest == NATIVE_CONFIG:
            assert os.name == 'nt' and len(raw) == 374
        elif row['path'] == 'src/user/windows/native-runtime.md' and digest != row['sha256']:
            update = load(validation / 'engineering-source-updates.json')
            assert update['path'] == row['path'] and update['before-sha256'] == row['sha256']
            assert update['after-sha256'] == digest and update['after-bytes'] == len(raw)
            assert hashlib.sha256(raw[:row['bytes']]).hexdigest() == row['sha256']
        else:
            assert len(raw) == row['bytes'] and digest == row['sha256'], row['path']
    baseline = load(validation / 'safeboot-native-baseline-a.json')
    assert baseline['state'] == 'expected-regression-failed' and baseline['exit-code'] == 101
    assert baseline['passed'] == 0 and baseline['failed'] == 1
    raw = (validation / baseline['log']).read_bytes()
    assert len(raw) == baseline['log-bytes'] and hashlib.sha256(raw).hexdigest() == baseline['log-sha256']
    assert b'0 passed; 1 failed;' in raw and b'assertion failed: matches!(registry.lookup(&guest), super::super::Lookup::Missing)' in raw
    prototype = load(validation / 'loader-macos-prototype-epoch3.json')
    raw = (validation / prototype['log']).read_bytes()
    assert len(raw) == prototype['bytes'] and hashlib.sha256(raw).hexdigest() == prototype['sha256']
    assert b'18 passed; 0 failed;' in raw
    for host, counts in COUNTS.items():
        record = load(validation / ('loader-native-epoch3b-gates.json' if host == 'native' else f'loader-{host}-epoch3-gates.json'))
        assert record['state'] == 'finished' and record['exit-code'] == 0
        steps = {row['name']: row for row in record['steps']}
        assert len(steps) == len(record['steps'])
        assert {'full', 'capi', 'integration', 'all-targets'} <= steps.keys()
        full = bound_log(validation, steps['full'])
        summary(full, *counts)
        if host == 'native':
            assert steps['full']['exit-code'] == 101 and set(record['exact-existing-failures']) == FAILURES
            final = full[full.rfind('\nfailures:\n'):]
            assert set(re.findall(r'^    (\S+)\r?$', final, re.M)) == FAILURES
            focus = bound_log(validation, steps['focus'])
            assert '27 passed; 0 failed;' in focus
            names = re.findall(r'^test (\S*loader_registry\S*) \.{3} ', focus, re.M)
            assert len(names) == len(set(names)) == 27
            leaves = [name.rsplit('::', 1)[-1] for name in names]
            for prefix in ['loader_registry_', 'native_loader_registry_', 'installed_loader_registry_']:
                assert sum(name.startswith(prefix) for name in leaves) == 9
        else:
            assert steps['full']['exit-code'] == 0
        for name in ['capi', 'integration', 'all-targets']:
            assert steps[name]['exit-code'] == 0
        capi = bound_log(validation, steps['capi'])
        summary(capi, 183, 0, 0)
        integrations = bound_log(validation, steps['integration'])
        summary(integrations, 4 if host == 'native' else 544, 0, 0)
        build_log = bound_log(validation, steps['all-targets'])
        assert 'Finished `dev` profile' in build_log
        for name in ['full', 'capi', 'integration', 'all-targets']:
            argv = steps[name]['argv']
            assert '--locked' in argv and '--offline' in argv and '--no-default-features' in argv
        assert '--lib' in steps['full']['argv']
        assert '--all-targets' in steps['all-targets']['argv'] and 'check' in steps['all-targets']['argv']
    consumers = [('macos', 'macos-loader-epoch3-owning-gates.json',
                  'macos-loader-epoch3-pre-cpp-archive-identities.json',
                  'macos-loader-epoch3-final-owning-archive-identities.json'),
                 ('linux', 'linux-loader-epoch3-owning-gates.json',
                  'linux-loader-epoch3-pre-cpp-archive-identities.json',
                  'linux-loader-epoch3-final-owning-archive-identities.json'),
                 ('windows', 'loader-native-owning-epoch3.json',
                  'native-loader-epoch3-pre-cpp-archives.json',
                  'native-loader-epoch3-final-owning-archive-identities.json')]
    for host, name, before_name, after_name in consumers:
        record = load(validation / name)
        assert record['state'] == 'finished' and record['exit-code'] == 0
        assert record.get('source-inputs', record.get('verified-source-inputs')) == 528
        texts = {}
        for row in record['steps']:
            assert row['exit-code'] == 0
            texts[row.get('name', row.get('gate'))] = bound_log(validation, row)
        before, after = load(validation / before_name), load(validation / after_name)
        assert before['archives'] == after['archives'] and after['core-features'] == []
        assert len(after['archives']) == 2 and len(after['sources']) == 528
        selected = load(validation / 'loader-owning-source-hashes-epoch3.json')
        assert before['sources'] == after['sources'] == selected['sources']
        for row in selected['sources']:
            raw = (source / row['path']).read_bytes()
            assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], row['path']
        for row in after['archives']:
            assert row['bytes'] > 0 and row['members'] > 0
            assert row['structural-member-walk'] == 'pass' and re.fullmatch('[a-f0-9]{64}', row['sha256'])
        if host == 'macos':
            ctest = texts['ctest']
            selected = ['assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
                        'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
                        'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test']
            rows = re.findall(r'(?m)^\s*(\d+)/7 Test\s+#\d+: (\S+)\s+\.+\s+Passed\s+', ctest)
            assert {int(index) for index, _ in rows} == set(range(1, 8)) and len(rows) == 7
            assert {name for _, name in rows} == set(selected)
            assert '100% tests passed' in ctest
            linkage = load(validation / 'macos-loader-epoch3-linkage.json')
            expected_targets = {'assist_rax_process_tests', 'assist_process_tool_tests', 'assist_rust_archive_link_tests',
                                'assist_e14_rax_abi_drift_tests', 'assist_mcp', 'assist_idalib_cli'}
            assert len(linkage) == 6 and {row['target'] for row in linkage} == expected_targets
            assert all(row['carrier'] == after['archives'][0]['path'] for row in linkage)
        else:
            cpp = texts['cpp']
            assert 'linked-archive-before' in cpp and 'linked-archive-after' in cpp
            assert after['archives'][0]['sha256'] in cpp
            for consumer in ['process_tool', 'process_tool_unavailable', 'rax_process', 'rax_abi_drift', 'rust_archive_link']:
                assert re.search(r'^run (\S*[/\\]' + consumer + r'(?:\.exe)?) \1\r?$', cpp, re.M)
    controls = load(validation / 'loader-native-negative-controls.json')
    assert controls['all-rejected'] is True and len(controls['controls']) == 18
    assert all(row['rejected'] for row in controls['controls'])
    ordinary = load(validation / 'ordinary-program-summary.json')
    native_owner = load(validation / 'native-loader-epoch3-final-owning-archive-identities.json')
    core = next(row for row in native_owner['archives'] if row['path'].endswith('.rlib'))
    assert ordinary['actual-core'] == core
    observer = load(validation / 'loader-owner-epoch3-launch.json')
    assert observer['state'] == 'finished' and observer['exit_code'] == 0
    assert observer['source-before'] == observer['source-after']
    assert observer['source-before']['sha256'] == 'dbd5bbc130c82fe7b42dbca93c53fbff26b3c061cd9a31f1376cd05b1d8617ed'
    assert observer['executable-before'] == observer['executable-after']
    assert observer['archive']['sha256'] == observer['archive-after']['sha256'] == core['sha256']
    assert observer['runtime-modules-before'] == observer['runtime-modules-after']
    assert len(observer['runtime-modules-before']) == 6 and len(ordinary['programs']) == 2
    assert len(observer['outcomes']) == 2
    hashes = ['325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
              '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6']
    expected_stdout = ['', '6e7420617574686f726974795c73797374656d0d0a']
    for index, (row, observed, previous, digest) in enumerate(zip(ordinary['programs'], observer['outcomes'], [2883900,2886185], hashes)):
        assert observed['program-before'] == observed['program-after']
        assert observed['program-before']['sha256'] == digest
        assert observed['native-before'] == observed['native-after']
        assert observed['native-before'] == {'exit-code': 0, 'stdout-hex': expected_stdout[index], 'stderr-hex': ''}
        assert observed['exit-code'] == row['controller-exit-code'] == 0
        assert row['former-stop-turn'] == previous
        raw = (validation / row['log']).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == row['log-sha256'] == observed['log-identity']['sha256']
        text = raw.decode('utf-8')
        assert 'prefix-turns=2100000 result=BudgetExhausted' in text
        for offset, expected in [(0,'c0000034'),(15,'c0000034'),(23,'0')]:
            assert f'registry-result turn={previous+offset} tid=8 status=Some({expected})' in text
        terminal = re.search(r'^terminal turn=(\d+) (.+)$',text,re.M)
        assert terminal and int(terminal.group(1)) == previous+74 == row['new-terminal-turn']
        assert terminal.group(2) == row['terminal-result']
        assert 'NtQueryInformationToken class 1' in terminal.group(2)
        assert row['additional-observer-turns'] == 74 and row['guest-exit-success'] is False
        assert text.count('guest-console ') == 2
    assert ordinary['ordinary-guest-exit-success'] is False
    return {'source-inputs': len(source_rows), 'three-host-full-gates': 'pass', 'retained-consumers': 'pass',
            'ordinary-programs': 2, 'ordinary-guest-exit-success': ordinary['ordinary-guest-exit-success']}



def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('--evidence', type=Path)
    args = parser.parse_args()
    evidence = args.evidence or args.source / 'docs/specifications/windows/native-loader-registry'
    print(json.dumps(check(args.source, evidence), indent=2))


if __name__ == '__main__':
    main()
