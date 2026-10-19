"""Check source identity, actual full selections and retained consumer evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re

COUNTS = {'macos': (8049, 0, 2), 'linux': (8043, 0, 2), 'native': (7512, 5, 2)}
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
    receipts_path = validation / 'proof-receipts.json'
    if receipts_path.exists():
        receipts = load(receipts_path)
        checked = (validation / 'checked-evidence-envelope.json').read_bytes()
        assert hashlib.sha256(checked).hexdigest() == receipts['checked-envelope-sha256']
        original = json.loads(checked)
        assert len(original['files']) == receipts['checked-envelope-files']
        for item in original['files']:
            raw = (evidence / item['path']).read_bytes()
            assert len(raw) == item['bytes'] and hashlib.sha256(raw).hexdigest() == item['sha256']
        assert {row['host'] for row in receipts['receipts']} == {'macos', 'linux', 'windows'}
        assert len(receipts['receipts']) == 3
        for item in receipts['receipts']:
            receipt = load(validation / item['receipt'])
            assert receipt['state'] == 'finished' and receipt['exit-code'] == 0
            assert receipt['checked-envelope-files'] == item['checked-envelope-files'] == receipts['checked-envelope-files']
            bound_log(validation, receipt)
    import runpy, shutil, tempfile
    scratch = source / 'build-tmp'
    scratch.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='attributes-proof-', dir=scratch) as temp:
        temporary = Path(temp)
        for name in ['cases.json', 'empty-cases.json', 'upper-cases.json']:
            shutil.copy2(evidence / name, temporary / name)
        entry = runpy.run_path(str(evidence / 'check_layouts.py'))['main']
        entry.__globals__['HERE'] = temporary
        assert entry() == 0
        native = load(temporary / 'native-layout-check.json')
        assert native['checked'] == 22656 and native['zero-value-creation-controls'] == 28
    primary = load(evidence / 'primary/phnt-source.json')
    assert primary['url'] == 'https://raw.githubusercontent.com/winsiderss/phnt/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntseapi.h'
    raw = (evidence / 'primary/phnt-ntseapi.h').read_bytes()
    assert len(raw) == primary['bytes'] == 74208
    assert hashlib.sha256(raw).hexdigest() == primary['sha256'] == '7fa0a21902ed64036c495216fba85d0e29a1d3028fd60d09103993817d1473df'
    manifest = load(validation / 'attributes-candidate-epoch2.json')
    rows = manifest['files']
    assert len(rows) == 24786 and sum(row['bytes'] for row in rows) == 1808385701
    assert len({row['path'] for row in rows}) == len(rows)
    assert not any('__pycache__' in Path(row['path']).parts for row in rows)
    assert manifest['base-repository-commit'] == 'd03afec80afd8ef4281c839a8944a166aef02369'
    source_rows = rows
    updates = {row['path']: row for row in load(validation / 'engineering-source-updates.json')['files']}
    for row in rows:
        path = source / row['path']
        raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        if row['path'] == '.cargo/config.toml' and digest == NATIVE_CONFIG:
            assert os.name == 'nt' and len(raw) == 374
        elif row['path'] in updates:
            update = updates[row['path']]
            assert (row['path'].startswith('docs/specifications/windows/native-token-security-attributes/')
                    or row['path'] == 'src/user/windows/native-runtime.md')
            assert update['before-sha256'] == row['sha256'] and update['before-bytes'] == row['bytes']
            assert update['after-sha256'] == digest and update['after-bytes'] == len(raw)
            if row['path'] == 'src/user/windows/native-runtime.md':
                assert hashlib.sha256(raw[:row['bytes']]).hexdigest() == row['sha256']
        else:
            assert len(raw) == row['bytes'] and digest == row['sha256'], row['path']
    baseline = load(validation / 'attributes-baseline-proof.json')
    assert baseline['state'] == 'expected-regression-failed'
    assert baseline['passed'] == 0 and baseline['failed'] == 3
    raw_record = (validation / baseline['record']).read_bytes()
    assert hashlib.sha256(raw_record).hexdigest() == baseline['record-sha256']
    observed = load(validation / baseline['record'])
    assert observed['exit-code'] == 101
    raw = (validation / observed['log']).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == observed['sha256'] and len(raw) == observed['bytes']
    assert b'0 passed; 3 failed;' in raw and b'Finished `test` profile' in raw and b'error[E' not in raw
    for name in ['empty', 'upper']:
        proof = load(validation / ('attributes-' + name + '-baseline-proof.json'))
        assert proof['state'] == 'expected-regression-failed' and proof['passed'] == 0 and proof['failed'] == 1 and proof['exit-code'] == 101
        raw = (validation / ('attributes-' + name + '-baseline-epoch2.log')).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == proof['log-sha256']
        assert b'0 passed; 1 failed;' in raw and b'Finished `test` profile' in raw and b'error[E' not in raw
    prototype = load(validation / 'attributes-prototype-epoch2-e.json')
    assert prototype['state'] == 'finished' and prototype['exit-code'] == 0
    raw = (validation / prototype['log']).read_bytes()
    assert len(raw) == prototype['bytes'] and hashlib.sha256(raw).hexdigest() == prototype['sha256']
    assert b'12 passed; 0 failed;' in raw
    for row in prototype['before-and-after-inputs']:
        raw = (source / row['path']).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], row['path']
    formatting = load(validation / 'attributes-workspace-format.json')
    assert formatting['exit-code'] == 0 and len(formatting['unchanged-owned-rust-inputs']) == 18
    bound_log(validation, formatting)
    for item in formatting['unchanged-owned-rust-inputs']:
        raw = (source / item['path']).read_bytes()
        assert len(raw) == item['bytes'] and hashlib.sha256(raw).hexdigest() == item['sha256']
    for host, counts in COUNTS.items():
        record = load(validation / ('attributes-native-epoch2-gates.json' if host == 'native' else f'attributes-{host}-epoch2-gates.json'))
        assert record['state'] == 'finished' and record['exit-code'] == 0
        steps = {row['name']: row for row in record['steps']}
        assert len(steps) == len(record['steps'])
        assert {'full', 'capi', 'integration', 'all-targets'} <= steps.keys()
        full = bound_log(validation, steps['full'])
        summary(full, *counts)
        ignored = set(re.findall(r'^test (\S+) \.{3} ignored\r?$', full, re.M))
        assert ignored == {'smir::lower::validation::tests::test_lift_lower_full_microkernel', 'smir::lower::validation::tests::test_roundtrip_exact_bytes_microkernel'}
        if host == 'native':
            assert steps['full']['exit-code'] == 101 and set(record['exact-existing-failures']) == FAILURES
            final = full[full.rfind('\nfailures:\n'):]
            assert set(re.findall(r'^    (\S+)\r?$', final, re.M)) == FAILURES
            focus = bound_log(validation, steps['focus'])
            assert '13 passed; 0 failed;' in focus
            names = re.findall(r'^test (\S*native_token_security_attributes\S*) \.{3} ', focus, re.M)
            assert len(names) == len(set(names)) == 13
            assert any('installed_leaves' in name for name in names)
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
    consumers = [('macos', 'macos-attributes-epoch2-owning-gates-c.json',
                  'macos-attributes-epoch2-c-pre-cpp-archive-identities.json',
                  'macos-attributes-epoch2-c-final-owning-archive-identities.json'),
                 ('linux', 'linux-attributes-epoch2-owning-gates-b.json',
                  'linux-attributes-epoch2-pre-cpp-archive-identities.json',
                  'linux-attributes-epoch2-final-owning-archive-identities.json'),
                 ('windows', 'attributes-native-owning-epoch2-d.json',
                  'native-attributes-epoch2-d-pre-cpp-archives.json',
                  'native-attributes-epoch2-d-final-owning-archive-identities.json')]
    for host, name, before_name, after_name in consumers:
        record = load(validation / name)
        assert record['state'] == 'finished' and record['exit-code'] == 0
        assert record.get('source-inputs', record.get('verified-source-inputs')) == 880
        texts = {}
        for row in record['steps']:
            assert row['exit-code'] == 0
            texts[row.get('name', row.get('gate'))] = bound_log(validation, row)
        before, after = load(validation / before_name), load(validation / after_name)
        assert before['archives'] == after['archives'] and after['core-features'] == []
        assert len(after['archives']) == 2 and len(after['sources']) == 880
        selected = load(validation / 'attributes-owning-source-hashes-epoch2.json')
        assert before['sources'] == after['sources'] == selected['sources']
        for row in selected['sources']:
            raw = (source / row['path']).read_bytes()
            assert (row['path'] in updates or (len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'])), row['path']
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
            linkage = load(validation / 'macos-attributes-epoch2-c-linkage.json')
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
    controls = load(validation / 'attributes-oracle-negative-results.json')
    assert len(controls['controls']) == 16
    assert controls['controls'][0]['exit-code'] == 0
    assert all(row['exit-code'] != 0 for row in controls['controls'][1:])
    ordinary = check_ordinary(validation)
    return {'source-inputs': len(source_rows), 'three-host-full-gates': 'pass', 'retained-consumers': 'pass',
            'ordinary-programs': 2, 'ordinary-guest-exit-success': ordinary['ordinary-guest-exit-success']}


def check_ordinary(validation):
    ordinary = load(validation / 'ordinary-program-summary.json')
    native_owner = load(validation / 'native-attributes-epoch2-d-final-owning-archive-identities.json')
    core = next(row for row in native_owner['archives'] if row['path'].endswith('.rlib'))
    assert ordinary['actual-core'] == core
    observer = load(validation / 'attributes-owner-epoch2-launch.json')
    assert observer['state'] == 'finished' and observer['exit_code'] == 0
    assert observer['source-before'] == observer['source-after']
    saved_observer = (validation / 'attributes-owner-epoch2.rs').read_bytes()
    assert len(saved_observer) == observer['source-before']['bytes']
    assert hashlib.sha256(saved_observer).hexdigest() == observer['source-before']['sha256']
    assert observer['source-before']['sha256'] == '684d908a33165a0479bbbf84c7ea55f88e3cc4325ce8e3a7845df6e4553a9154'
    assert observer['executable-before'] == observer['executable-after']
    assert observer['archive']['sha256'] == observer['archive-after']['sha256'] == core['sha256']
    assert observer['runtime-modules-before'] == observer['runtime-modules-after']
    assert len(observer['runtime-modules-before']) == 6 and len(ordinary['programs']) == 2
    assert len(observer['outcomes']) == 2
    hashes = ['325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
              '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6']
    expected_stdout = ['', '6e7420617574686f726974795c73797374656d0d0a']
    for index, (row, observed, previous, digest) in enumerate(zip(ordinary['programs'], observer['outcomes'], [2899173, 2901458], hashes)):
        assert observed['program-before'] == observed['program-after']
        assert observed['program-before']['sha256'] == digest
        assert observed['native-before'] == observed['native-after']
        assert observed['native-before'] == {'exit-code': 0, 'stdout-hex': expected_stdout[index], 'stderr-hex': ''}
        assert observed['exit-code'] == row['controller-exit-code'] == 0
        assert row['former-stop-turn'] == previous
        raw = (validation / row['log']).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == row['log-sha256'] == observed['log-identity']['sha256']
        assert len(raw) == observed['log-identity']['bytes']
        text = raw.decode('utf-8')
        assert 'prefix-turns=2100000 result=BudgetExhausted' in text
        for turn in [previous, previous + 16]:
            line = next(line for line in text.splitlines() if line.startswith(f'token-security-attributes-result turn={turn} tid=8 '))
            assert 'status=Some(c0000225)' in line and 'required=Ok(0)' in line and 'result=BudgetExhausted' in line
        terminal = re.search(r'^terminal turn=(\d+) (.+)$', text, re.M)
        delta = [44004, 64739][index]
        expected = ['NtTestAlert (0x1d3)', 'NtQueryVirtualMemory class 3'][index]
        assert terminal and int(terminal.group(1)) == previous + delta == row['new-terminal-turn']
        assert terminal.group(2) == row['terminal-result'] and expected in terminal.group(2)
        assert row['additional-observer-turns'] == delta and row['guest-exit-success'] is False
        assert text.count('guest-console ') == 2
        for stream in ['stdout', 'stderr']:
            assert re.search(r'guest-console .* stream=' + stream + r' bytes=0 hex=\r?$', text, re.M)
    assert ordinary['ordinary-guest-exit-success'] is False
    return ordinary




def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('--evidence', type=Path)
    args = parser.parse_args()
    evidence = args.evidence or args.source / 'docs/specifications/windows/native-token-security-attributes'
    print(json.dumps(check(args.source, evidence), indent=2))


if __name__ == '__main__':
    main()
