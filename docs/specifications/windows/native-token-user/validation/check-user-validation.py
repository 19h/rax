"""Check source identity, actual full selections and retained consumer evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re

COUNTS = {'macos': (8013, 0, 2), 'linux': (8007, 0, 2), 'native': (7467, 5, 2)}
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


def check_teb(validation):
    stem = 'native-token-user-teb-v1'
    digest = '01beda923ed6c3c2560369123ca9aae9bec2ace49898afa5132a6081e91a79c1'
    assert hashlib.sha256((validation / (stem + '.cpp')).read_bytes()).hexdigest() == digest
    assert digest in (validation / (stem + '.ps1')).read_text()
    launch = load(validation / (stem + '-launch.json'))
    assert launch['state'] == 'finished' and launch['exit-code'] == 0
    outcomes = load(validation / (stem + '-outcomes.json'))
    profiles = ['arm64', 'x64', 'x86', 'x86-laa']
    assert [row['profile'] for row in outcomes] == profiles and all(row['exit-code'] == 0 for row in outcomes)
    assert load(validation / (stem + '-modules-before.json')) == load(validation / (stem + '-modules-after.json'))
    calls = 0
    for profile, machine, laa in [('arm64',43620,True),('x64',34404,True),('x86',332,False),('x86-laa',332,True)]:
        prefix = stem + '-' + profile
        pe = load(validation / (prefix + '-before.json'))
        assert pe == load(validation / (prefix + '-after.json'))
        assert pe['machine'] == machine and pe['large-address-aware'] is laa
        assert load(validation / (prefix + '-source-after.json'))['Hash'].lower() == digest
        lines = (validation / (prefix + '.log')).read_text().splitlines()
        assert len(lines) == 9
        for index, line in enumerate(lines):
            row = dict(part.split('=',1) for part in line.split())
            role, length = int(row['role']), int(row['length'],16)
            assert role == index//3 and length == [0,28,0xFFFFFFFF][index%3]
            assert row['profile'] == profile and row['ntstatus'] == 'C0000034'
            wow = profile.startswith('x86')
            if role == 1 and length != 0: status = 0xC0000005
            elif length == 0xFFFFFFFF: status = 0xC0000017 if wow else 0xC0000005
            elif role == 2: status = 0xC0000008
            elif length == 0: status = 0xC0000023
            else: status = 0
            assert int(row['status'],16) == status
            assert int(row['error'],16) == (8 if wow and status == 0xC0000017 else 0x7E11ABCD)
            required = (20 if wow else 28) if status in [0,0xC0000023] else (0xFFFFFFF8 if wow and status != 0xC0000017 else 0xA5A5A5A5)
            assert int(row['returned'],16) == required
            calls += 1
    assert calls == 36


def check(source, evidence):
    validation = evidence / 'validation'
    check_teb(validation)
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
    primary = load(evidence / 'capture-plan-v4.json')
    assert primary['expected-cases'] == 22688
    for row in primary['primary-sources']:
        raw = (evidence / row['file']).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
    host_identity = load(validation / 'timer2-native-host-identity.json')
    assert host_identity['machine'] == 'ARM64' and host_identity['pointer-bytes'] == 8
    assert host_identity['windows-version']['platform-version'] == [10, 0, 29683]
    assert host_identity['runtime-modules'][0]['sha256'] == '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
    raw_identity_source = (validation / 'capture-timer2-host-identity.py').read_bytes()
    assert hashlib.sha256(raw_identity_source).hexdigest() == host_identity['source-sha256']
    manifest = load(validation / 'user-candidate-epoch1.json')
    source_rows = manifest['files']
    rows = source_rows
    assert len(rows) == 23442 and sum(row['bytes'] for row in rows) == 1695720818
    assert len({row['path'] for row in rows}) == len(rows)
    assert manifest['base-repository-commit'] == '4be14abe7c2dcb238bba92cb6779a24d8988d801'
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
    baseline = load(validation / 'user-baseline-b.json')
    assert baseline['state'] == 'expected-regression-failed' and baseline['exit-code'] == 101
    assert baseline['passed'] == 0 and baseline['failed'] == 5
    raw = (validation / baseline['log']).read_bytes()
    assert len(raw) == baseline['bytes'] and hashlib.sha256(raw).hexdigest() == baseline['sha256']
    assert b'0 passed; 5 failed;' in raw and b'NtQueryInformationToken class 1' in raw
    prototype = load(validation / 'user-prototype-a.json')
    assert prototype['state'] == 'finished' and prototype['exit-code'] == 0
    raw = (validation / prototype['log']).read_bytes()
    assert len(raw) == prototype['bytes'] and hashlib.sha256(raw).hexdigest() == prototype['sha256']
    assert b'5 passed; 0 failed;' in raw
    for row in prototype['inputs']:
        raw = (source / row['path']).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
    for host, counts in COUNTS.items():
        record = load(validation / ('user-native-epoch1-gates.json' if host == 'native' else f'user-{host}-epoch1-gates.json'))
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
            assert '6 passed; 0 failed;' in focus
            names = re.findall(r'^test (\S*native_token_user\S*) \.{3} ', focus, re.M)
            assert len(names) == len(set(names)) == 6
            assert any('installed_leaves_match_independent_host_sid_and_cleanup' in name for name in names)
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
    consumers = [('macos', 'macos-user-epoch1-owning-gates.json',
                  'macos-user-epoch1-pre-cpp-archive-identities.json',
                  'macos-user-epoch1-final-owning-archive-identities.json'),
                 ('linux', 'linux-user-epoch1-owning-gates.json',
                  'linux-user-epoch1-pre-cpp-archive-identities.json',
                  'linux-user-epoch1-final-owning-archive-identities.json'),
                 ('windows', 'user-native-owning-epoch1.json',
                  'native-user-epoch1-pre-cpp-archives.json',
                  'native-user-epoch1-final-owning-archive-identities.json')]
    for host, name, before_name, after_name in consumers:
        record = load(validation / name)
        assert record['state'] == 'finished' and record['exit-code'] == 0
        assert record.get('source-inputs', record.get('verified-source-inputs')) == 530
        texts = {}
        for row in record['steps']:
            assert row['exit-code'] == 0
            texts[row.get('name', row.get('gate'))] = bound_log(validation, row)
        before, after = load(validation / before_name), load(validation / after_name)
        assert before['archives'] == after['archives'] and after['core-features'] == []
        assert len(after['archives']) == 2 and len(after['sources']) == 530
        selected = load(validation / 'user-owning-source-hashes-epoch1.json')
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
            linkage = load(validation / 'macos-user-epoch1-linkage.json')
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
    controls = load(evidence / 'native-user-negative-controls.json')
    assert controls['all-rejected'] is True and len(controls['controls']) == 12
    assert all(row['rejected'] for row in controls['controls'])
    ordinary = load(validation / 'ordinary-program-summary.json')
    native_owner = load(validation / 'native-user-epoch1-final-owning-archive-identities.json')
    core = next(row for row in native_owner['archives'] if row['path'].endswith('.rlib'))
    assert ordinary['actual-core'] == core
    observer = load(validation / 'user-owner-epoch1-launch.json')
    assert observer['state'] == 'finished' and observer['exit_code'] == 0
    assert observer['source-before'] == observer['source-after']
    assert observer['source-before']['sha256'] == 'c5b4c58bd15223f91bfa9bd48552e90462b5d977dbda731326af6f01e68506ed'
    assert observer['executable-before'] == observer['executable-after']
    assert observer['archive']['sha256'] == observer['archive-after']['sha256'] == core['sha256']
    assert observer['runtime-modules-before'] == observer['runtime-modules-after']
    assert len(observer['runtime-modules-before']) == 6 and len(ordinary['programs']) == 2
    assert len(observer['outcomes']) == 2
    hashes = ['325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
              '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6']
    expected_stdout = ['', '6e7420617574686f726974795c73797374656d0d0a']
    for index, (row, observed, previous, digest) in enumerate(zip(ordinary['programs'], observer['outcomes'], [2883974,2886259], hashes)):
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
        assert f'token-user-call turn={previous} tid=8' in text
        assert f'token-user-result turn={previous} tid=8 status=Some(0)' in text
        assert 'required=Ok(1c)' in text
        terminal = re.search(r'^terminal turn=(\d+) (.+)$', text, re.M)
        assert terminal and int(terminal.group(1)) == previous+2743 == row['new-terminal-turn']
        assert terminal.group(2) == row['terminal-result']
        assert 'NtOpenKey: registry key outside selected runtime snapshot' in terminal.group(2)
        assert 'Safer' in text and 'S-1-5-18' in text
        assert row['additional-observer-turns'] == 2743 and row['guest-exit-success'] is False
        assert text.count('guest-console ') == 2
    assert ordinary['ordinary-guest-exit-success'] is False
    return {'source-inputs': len(source_rows), 'three-host-full-gates': 'pass', 'retained-consumers': 'pass',
            'ordinary-programs': 2, 'ordinary-guest-exit-success': ordinary['ordinary-guest-exit-success']}



def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('--evidence', type=Path)
    args = parser.parse_args()
    evidence = args.evidence or args.source / 'docs/specifications/windows/native-token-user'
    print(json.dumps(check(args.source, evidence), indent=2))


if __name__ == '__main__':
    main()
