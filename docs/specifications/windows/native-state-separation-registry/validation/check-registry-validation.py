"""Check source identity, actual full selections and retained consumer evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re

COUNTS = {'macos': (7976, 0, 2), 'linux': (7970, 0, 2), 'native': (7419, 5, 2)}
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
    manifest = load(validation / 'registry-source-epoch1.json')
    source_rows = manifest['files']
    rows = source_rows
    assert len(rows) == 22779 and sum(row['bytes'] for row in rows) == 1583474670
    assert len({row['path'] for row in rows}) == len(rows)
    assert manifest['base-repository-commit'] == 'daecebd58bd574fd95281ff2635b0754f6e1dede'
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
    baseline = load(validation / 'registry-native-baseline-b.json')
    assert baseline['state'] == 'expected-regression-failed' and baseline['exit-code'] == 101
    assert any('0 passed; 1 failed;' in result for result in baseline['results'])
    baseline_log = (validation / 'registry-native-baseline-b.log').read_bytes()
    assert hashlib.sha256(baseline_log).hexdigest() == baseline['log-sha256']
    assert b'assertion failed: matches!(registry.lookup(&guest)' in baseline_log
    prototype = load(validation / 'registry-macos-prototype.json')
    assert prototype['state'] == 'finished' and prototype['exit-code'] == 0
    assert prototype['inputs-before'] == prototype['inputs-after']
    assert [(step['name'], step['exit-code']) for step in prototype['steps']] == [('new', 0), ('existing-ifeo', 0)]
    for host, counts in COUNTS.items():
        record = load(validation / f'registry-{host}-epoch1-gates.json')
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
            assert '9 passed; 0 failed;' in focus
            assert 'native controlled redirection capture: 54 keys, 106 values, 4756 bytes' in focus
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
    consumers = [('macos', 'macos-registry-epoch1-owning-gates.json',
                  'macos-registry-epoch1-pre-cpp-archive-identities.json',
                  'macos-registry-epoch1-final-owning-archive-identities.json'),
                 ('linux', 'linux-registry-epoch1-owning-gates.json',
                  'linux-registry-epoch1-pre-cpp-archive-identities.json',
                  'linux-registry-epoch1-final-owning-archive-identities.json'),
                 ('windows', 'registry-native-owning-epoch1.json',
                  'native-registry-epoch1-pre-cpp-archives.json',
                  'native-registry-epoch1-final-owning-archive-identities.json')]
    for host, name, before_name, after_name in consumers:
        record = load(validation / name)
        assert record['state'] == 'finished' and record['exit-code'] == 0
        assert record.get('source-inputs', record.get('verified-source-inputs')) == 513
        texts = {}
        for row in record['steps']:
            assert row['exit-code'] == 0
            texts[row.get('name', row.get('gate'))] = bound_log(validation, row)
        before, after = load(validation / before_name), load(validation / after_name)
        assert before['archives'] == after['archives'] and after['core-features'] == []
        assert len(after['archives']) == 2 and len(after['sources']) == 513
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
            linkage = load(validation / 'macos-registry-epoch1-linkage.json')
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
    ordinary = load(validation / 'ordinary-program-summary.json')
    native_owner = load(validation / 'native-registry-epoch1-final-owning-archive-identities.json')
    core = next(row for row in native_owner['archives'] if row['path'].endswith('.rlib'))
    assert ordinary['actual-core'] == core
    assert ordinary['ordinary-guest-exit-success'] is False
    observer = load(validation / 'redirection-owner-epoch1-launch.json')
    assert observer['state'] == 'finished' and observer['exit_code'] == 0
    assert observer['source-before'] == observer['source-after']
    assert observer['source-before']['sha256'] == 'f73f278d4b8e53417453c4cbe06a6eead338710653e5249a0d35d96c30be133a'
    assert observer['executable-before'] == observer['executable-after']
    assert observer['archive']['sha256'] == observer['archive-after']['sha256'] == core['sha256']
    assert observer['runtime-modules-before'] == observer['runtime-modules-after']
    assert len(observer['runtime-modules-before']) == 6 and len(ordinary['programs']) == 2
    for row, previous, terminal in zip(ordinary['programs'], [2178927, 2181212], [2188223, 2190508]):
        assert row['former-stop-turn'] == previous and row['new-terminal-turn'] == terminal
        assert row['additional-observer-turns'] == terminal - previous == 9296
        assert row['registry-ntstatus'] == 'C0000034' and row['output-handle'] == 0
        assert row['guest-exit-success'] is False and row['controller-exit-code'] == 0
        raw = (validation / row['log']).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == row['log-sha256']
        text = raw.decode('utf-8')
        assert f'registry-result turn={previous} tid=8 status=Some(c0000034) output=Ok(0)' in text
        assert 'prefix-turns=2100000 result=BudgetExhausted' in text
        assert f'terminal turn={terminal} Complete(Internal("native NTDLL service NtSetTimer2 (0x1c0)' in text
    return {'source-inputs': len(source_rows), 'three-host-full-gates': 'pass', 'retained-consumers': 'pass',
            'ordinary-programs': 2, 'next-frontier': 'NtSetTimer2', 'ordinary-guest-exit-success': False}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('--evidence', type=Path)
    args = parser.parse_args()
    evidence = args.evidence or args.source / 'docs/specifications/windows/native-state-separation-registry'
    print(json.dumps(check(args.source, evidence), indent=2))


if __name__ == '__main__':
    main()
