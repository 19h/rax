"""Check source identity, actual full selections and retained consumer evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re

COUNTS = {'macos': (8031, 0, 2), 'linux': (8025, 0, 2), 'native': (7492, 5, 2)}
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
    import runpy
    native = runpy.run_path(str(evidence / 'check_native.py'))['check'](evidence)
    assert native['native-setter-cases'] == 2600 and native['native-interleaving-controls'] == 36 and native['bound-native-files'] == 64
    supplement = runpy.run_path(str(evidence / 'check_supplement.py'))['check'](evidence)
    assert supplement['native-second-stack-cases'] == 2600 and supplement['compiled-input-changes'] is False
    primary = load(evidence / 'primary-retrieval.json')
    assert len(primary['sources']) == 1
    for row in primary['sources']:
        assert row['url'].startswith('https://learn.microsoft.com/')
        raw = (evidence / row['file']).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
    host_identity = load(validation / 'timer2-native-host-identity.json')
    assert host_identity['machine'] == 'ARM64' and host_identity['pointer-bytes'] == 8
    assert host_identity['windows-version']['platform-version'] == [10, 0, 29683]
    assert host_identity['runtime-modules'][0]['sha256'] == '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
    raw_identity_source = (validation / 'capture-timer2-host-identity.py').read_bytes()
    assert hashlib.sha256(raw_identity_source).hexdigest() == host_identity['source-sha256']
    manifest = load(validation / 'flags-candidate-epoch1.json')
    source_rows = manifest['files']
    rows = source_rows
    assert len(rows) == 24045 and sum(row['bytes'] for row in rows) == 1731467640
    assert len({row['path'] for row in rows}) == len(rows)
    assert manifest['base-repository-commit'] == '9e243946c5c4e8df0bcbc93d94d1337ec8d62171'
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
    baseline = load(validation / 'flags-baseline-proof.json')
    assert baseline['state'] == 'expected-regression-failed' and baseline['exit-code'] == 101
    assert baseline['passed'] == 0 and baseline['failed'] == 3
    raw_record = (validation / baseline['record']).read_bytes()
    assert hashlib.sha256(raw_record).hexdigest() == baseline['record-sha256']
    observed = load(validation / baseline['record'])
    raw = (validation / baseline['log']).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == baseline['log-sha256'] == observed['sha256']
    assert len(raw) == observed['bytes'] and b'0 passed; 3 failed;' in raw
    assert b'Finished `test` profile' in raw and b'error[E' not in raw
    prototype = load(validation / 'flags-prototype-a.json')
    assert prototype['state'] == 'finished' and prototype['exit-code'] == 0
    raw = (validation / prototype['log']).read_bytes()
    assert len(raw) == prototype['bytes'] and hashlib.sha256(raw).hexdigest() == prototype['sha256']
    assert b'6 passed; 0 failed;' in raw
    for row in prototype['inputs']:
        raw = (source / row['path']).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
    for host, counts in COUNTS.items():
        record = load(validation / ('flags-native-epoch1-gates.json' if host == 'native' else f'flags-{host}-epoch1-gates.json'))
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
            assert '7 passed; 0 failed;' in focus
            names = re.findall(r'^test (\S*native_worker_factory_flags\S*) \.{3} ', focus, re.M)
            assert len(names) == len(set(names)) == 7
            assert any('native_worker_factory_flags_installed_leaf_updates_request_and_four_argument_cleanup' in name for name in names)
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
    consumers = [('macos', 'macos-flags-epoch1-owning-gates.json',
                  'macos-flags-epoch1-pre-cpp-archive-identities.json',
                  'macos-flags-epoch1-final-owning-archive-identities.json'),
                 ('linux', 'linux-flags-epoch1-owning-gates.json',
                  'linux-flags-epoch1-pre-cpp-archive-identities.json',
                  'linux-flags-epoch1-final-owning-archive-identities.json'),
                 ('windows', 'flags-native-owning-epoch1.json',
                  'native-flags-epoch1-pre-cpp-archives.json',
                  'native-flags-epoch1-final-owning-archive-identities.json')]
    for host, name, before_name, after_name in consumers:
        record = load(validation / name)
        assert record['state'] == 'finished' and record['exit-code'] == 0
        assert record.get('source-inputs', record.get('verified-source-inputs')) == 540
        texts = {}
        for row in record['steps']:
            assert row['exit-code'] == 0
            texts[row.get('name', row.get('gate'))] = bound_log(validation, row)
        before, after = load(validation / before_name), load(validation / after_name)
        assert before['archives'] == after['archives'] and after['core-features'] == []
        assert len(after['archives']) == 2 and len(after['sources']) == 540
        selected = load(validation / 'flags-owning-source-hashes-epoch1.json')
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
            linkage = load(validation / 'macos-flags-epoch1-linkage.json')
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
    controls = load(evidence / 'flags-native-negative-controls.json')
    assert controls['state'] == 'finished' and controls['rejected'] == len(controls['controls']) == 12
    assert all(row['rejected'] for row in controls['controls'])
    ordinary = check_ordinary(validation)
    return {'source-inputs': len(source_rows), 'three-host-full-gates': 'pass', 'retained-consumers': 'pass',
            'ordinary-programs': 2, 'ordinary-guest-exit-success': ordinary['ordinary-guest-exit-success']}


def check_ordinary(validation):
    ordinary = load(validation / 'ordinary-program-summary.json')
    native_owner = load(validation / 'native-flags-epoch1-final-owning-archive-identities.json')
    core = next(row for row in native_owner['archives'] if row['path'].endswith('.rlib'))
    assert ordinary['actual-core'] == core
    observer = load(validation / 'flags-owner-epoch1-launch.json')
    assert observer['state'] == 'finished' and observer['exit_code'] == 0
    assert observer['source-before'] == observer['source-after']
    assert observer['source-before']['sha256'] == 'f52a3911114dda42c281156e15c66afb6647cfa4408602f5f5a79d5163c9ff64'
    assert observer['executable-before'] == observer['executable-after']
    assert observer['archive']['sha256'] == observer['archive-after']['sha256'] == core['sha256']
    assert observer['runtime-modules-before'] == observer['runtime-modules-after']
    assert len(observer['runtime-modules-before']) == 6 and len(ordinary['programs']) == 2
    assert len(observer['outcomes']) == 2
    hashes = ['325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
              '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6']
    expected_stdout = ['', '6e7420617574686f726974795c73797374656d0d0a']
    for index, (row, observed, previous, digest) in enumerate(zip(ordinary['programs'], observer['outcomes'], [2898318,2900603], hashes)):
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
        assert f'worker-flags-call turn={previous} tid=8 args=[8c, d,' in text
        line = next(line for line in text.splitlines() if line.startswith(f'worker-flags-result turn={previous} tid=8 '))
        assert 'status=Some(0) result=BudgetExhausted' in line and 'flags_request: Some(1)' in line
        assert 'maximum: 512' in line and 'soft_maximum: None' in line and 'binding_count: 0' in line
        terminal = re.search(r'^terminal turn=(\d+) (.+)$', text, re.M)
        assert terminal and int(terminal.group(1)) == previous+496 == row['new-terminal-turn']
        assert terminal.group(2) == row['terminal-result']
        assert 'NtSetInformationWorkerFactory' in terminal.group(2) and 'class 2' in terminal.group(2)
        assert 'Safer' in text and 'S-1-5-18' in text and 'FileSystem' in text
        assert row['additional-observer-turns'] == 496 and row['guest-exit-success'] is False
        assert text.count('guest-console ') == 2
    assert ordinary['ordinary-guest-exit-success'] is False
    return ordinary



def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('--evidence', type=Path)
    args = parser.parse_args()
    evidence = args.evidence or args.source / 'docs/specifications/windows/native-worker-factory-flags'
    print(json.dumps(check(args.source, evidence), indent=2))


if __name__ == '__main__':
    main()
