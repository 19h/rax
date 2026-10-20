"""Check the immutable inputs, native capture, selected gates and consumer boundary."""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import os
import re

DOCS = Path(__file__).resolve().parent
VALIDATION = DOCS / 'validation'
SOURCE = DOCS.parents[3]
NATIVE_CONFIG = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
BASE = '6f2148297acc1e8106600306cc092b08207b80d2'
FAILURES = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}


_projection_path = SOURCE / 'tools/evidence/public_projection.py'
_projection_spec = importlib.util.spec_from_file_location('public_evidence_projection', _projection_path)
PROJECTION = importlib.util.module_from_spec(_projection_spec)
_projection_spec.loader.exec_module(PROJECTION)


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def identity(path):
    # Historical source/receipt identities remain exact. Public diagnostic
    # projections are separately bound to their original checkpoints.
    if path.is_symlink():
        raw = os.readlink(path).encode()
        return len(raw), hashlib.sha256(raw).hexdigest()
    return PROJECTION.recorded_identity(path)


def bound(row, path):
    assert PROJECTION.actual_identity(path) == (row['bytes'], row['sha256']) or identity(path) == (row['bytes'], row['sha256']), str(path)


def text(row):
    path = VALIDATION / row['log']
    bound(row, path)
    return path.read_text(errors='replace')


def finished(name):
    value = load(VALIDATION / name)
    assert value['state'] == 'finished' and value['exit-code'] == 0, name
    return value


def steps(value):
    return {row.get('name', row.get('gate')): row for row in value['steps']}


def fields(line):
    return dict(part.split('=', 1) for part in line.split() if '=' in part)


def native_capture():
    launch = finished('native-session-alias-v1-launch.json')
    source = load(VALIDATION / 'session-alias-native-source.json')
    bound(source, DOCS / 'native-session-alias-v1.cpp')
    assert launch['source-before'] == launch['source-after']
    assert (source['bytes'], source['sha256']) == (
        launch['source-before']['bytes'], launch['source-before']['sha256'])
    assert launch['runtime-modules-before'] == launch['runtime-modules-after']
    assert len(launch['runtime-modules-before']) == 6
    assert len(launch['steps']) == 16
    selected = steps(launch)
    for row in selected.values():
        assert row['exit-code'] == 0
        text(row)
    profiles = ['arm64', 'x64', 'x86', 'x86-laa']
    assert [row['profile'] for row in launch['outcomes']] == profiles
    for outcome, machine, width in zip(launch['outcomes'], [0xAA64, 0x8664, 0x14C, 0x14C], [8, 8, 4, 4]):
        profile = outcome['profile']
        assert outcome['exit-code'] == 0
        assert outcome['executable-before'] == outcome['executable-after']
        before = json.loads(text(selected[profile + '-before']))
        after = json.loads(text(selected[profile + '-after']))
        assert before == after and before['machine'] == machine
        assert before['large-address-aware'] == (profile != 'x86')
        assert (before['bytes'], before['sha256']) == (
            outcome['executable-before']['bytes'], outcome['executable-before']['sha256'])
        output = text(selected[profile])
        lines = output.splitlines()
        header = fields(lines[0])
        assert header['profile'] == profile and int(header['pointer']) == width
        assert int(header['attributes']) == (48 if width == 8 else 24)
        assert int(header['unicode']) == (16 if width == 8 else 8)
        decode = lambda value: bytes.fromhex(value).decode('utf-16-le')
        objects = {}
        private_entries = []
        fixed_entries = []
        for line in lines:
            if line.startswith('object '):
                row = fields(line)
                if row['role'].startswith('private-'):
                    objects[row['role']] = decode(row['name'])
            if line.startswith('entry role=private-parent '):
                row = fields(line)
                private_entries.append((decode(row['name']), decode(row['type'])))
            if line.startswith('entry role=fixed-root '):
                row = fields(line)
                fixed_entries.append((decode(row['name']), decode(row['type'])))
        assert set(fixed_entries) == {
            (name, 'Directory') for name in [
                'Sessions', 'Windows', 'KnownDllsChpe32', 'KnownDlls32',
                'BaseNamedObjects', 'KnownDlls']}
        assert len(fixed_entries) == 6
        assert private_entries == [('MiXeD', 'Directory')]
        assert objects['private-child'] == objects['private-alias']
        assert objects['private-child'].endswith('\\MiXeD')
        aliases = [fields(line) for line in lines if line.startswith('alias ')]
        assert len(aliases) == 1 and aliases[0]['status'] == '00000000'
        assert aliases[0]['flags'] == '00000040' and decode(aliases[0]['requested']) == 'mIxEd'
        symbolic = [fields(line) for line in lines if line.startswith('entry role=private-with-symbolic ')]
        assert {(decode(row['name']), decode(row['type'])) for row in symbolic} == {
            ('MiXeD', 'Directory'), ('DirectoryLink', 'SymbolicLink')}
        names = {fields(line)['role']: decode(fields(line)['name']) for line in lines if line.startswith('object ')}
        assert names['followed-directory'] == names['private-child']
        assert names['symbolic-link'] != names['private-child']
        comparison = [fields(line) for line in lines if line.startswith('identity ')]
        assert comparison == [{'same': '00000000', 'different': 'C00001AC', 'canonical-same': '1', 'link-name-distinct': '1'}]
        denied = [fields(line) for line in lines if line.startswith('dont-reparse ')]
        assert denied == [{'status': 'C000050B', 'opened': '0'}]
        ending = fields(lines[-1])
        assert ending['handle-count-before'] == ending['handle-count-after']
        assert lines[-1].startswith('finished=1 ')
    kernel = finished('kernel-profile.json')
    assert (kernel['major'], kernel['minor'], kernel['build'], kernel['native-processor-architecture']) == (10, 0, 29683, 12)
    return {'profiles': profiles, 'canonical-alias-checks': 4, 'kernel-build': 29683}


def consumers(source):
    owning = load(VALIDATION / 'session-alias-owning-source-hashes-epoch1.json')
    assert owning['epoch'] == 1 and len(owning['sources']) == 3826
    for row in owning['sources']:
        bound(row, source / row['path'])
    for row in owning['helpers']:
        bound(row, VALIDATION / row['path'])
    for platform in ['macos', 'linux', 'windows']:
        owner = finished('owning-' + platform + '.json')
        selected = steps(owner)
        assert len(selected) == (8 if platform == 'macos' else 5 if platform == 'linux' else 6)
        for row in selected.values():
            assert row['exit-code'] == 0
            text(row)
        before = load(VALIDATION / ('archives-' + platform + '-before.json'))
        after = load(VALIDATION / ('archives-' + platform + '-after.json'))
        assert before['archives'] == after['archives']
        assert before['sources'] == after['sources'] == owning['sources']
        assert before['core-features'] == after['core-features'] == []
        assert len(before['archives']) == 2
        assert all(row['structural-member-walk'] == 'pass' and row['members'] > 0 for row in before['archives'])
        if platform == 'macos':
            assert len(re.findall(r'^\d+/7 Test .*\sPassed\s', text(selected['ctest']), re.M)) == 7
            assert len(load(VALIDATION / 'macos-session-alias-epoch1-linkage.json')) == 6
    ordinary = finished('ordinary.json')
    capi = finished('capi.json')
    for receipt in [ordinary, capi]:
        for key in receipt:
            if key.endswith('-before'):
                assert receipt[key] == receipt[key[:-7] + '-after'], key
        for row in receipt['steps']:
            assert row['exit-code'] == 0
            text(row)
    windows = load(VALIDATION / 'archives-windows-after.json')
    identities = {(row['bytes'], row['sha256']) for row in windows['archives']}
    assert (ordinary['core-after']['bytes'], ordinary['core-after']['sha256']) in identities
    assert (capi['carrier-after']['bytes'], capi['carrier-after']['sha256']) in identities
    bound(ordinary['observer-after'], VALIDATION / 'session-alias-ordinary-owner-epoch2.rs')
    bound(capi['source-after'], VALIDATION / 'session-alias-owning-capi.cpp')
    chosen = steps(ordinary)
    smoke = text(chosen['smoke'])
    whoami = text(chosen['whoami'])
    assert 'Complete(Exited(0))' in smoke
    assert 'terminal-call=974' in smoke
    assert 'NtQueryAttributesFile (0x3d)' in whoami and 'terminal-call=2179' in whoami
    for outcome in ordinary['outcomes']:
        assert outcome['program-before'] == outcome['program-after']
        assert outcome['native-before'] == outcome['native-after']
        assert outcome['native-before']['exit-code'] == 0
    first, second = capi['outcomes']
    assert first['reason'] == 4 and first['exit-code'] == 0
    assert second['reason'] == 5 and 'NtQueryAttributesFile (0x3d)' in second['diagnostic']
    assert all(row['native-runtime'] and row['turns-started'] > 0 for row in capi['outcomes'])
    assert capi['native-after'][1]['stdout-hex'] == '6e7420617574686f726974795c73797374656d0d0a'
    retained = finished('retained-owning.json')
    assert len(retained['files']) == 4
    for row, expected in zip(retained['files'], [ordinary['core-after'], capi['carrier-after'],
                                               ordinary['executable-after'], capi['executable-after']]):
        assert (row['bytes'], row['sha256']) == (expected['bytes'], expected['sha256'])


def check(host, source=SOURCE):
    public = PROJECTION.check()
    assert public['projected-streams'] == 149
    envelope = load(DOCS / 'evidence-manifest.json')
    names = set()
    for row in envelope['files']:
        assert row['path'] not in names
        names.add(row['path'])
        bound(row, DOCS / row['path'])
    actual = {p.relative_to(DOCS).as_posix() for p in DOCS.rglob('*') if p.is_file()}
    assert actual == names | {'evidence-manifest.json'}
    frozen = load(VALIDATION / 'session-alias-source-epoch1.json')
    assert frozen['base-repository-commit'] == BASE and frozen['source-epoch'] == 1
    assert len(frozen['files']) == 25085 and frozen['total-bytes'] == 1835122521
    assert sum(row['bytes'] for row in frozen['files']) == frozen['total-bytes']
    for row in frozen['files']:
        value = identity(source / row['path'])
        assert value == (row['bytes'], row['sha256']) or (
            host == 'windows' and row['path'] == '.cargo/config.toml'
            and value == (374, NATIVE_CONFIG)), row['path']
    baseline = load(VALIDATION / 'session-alias-baseline-epoch2.json')
    assert baseline['exit-code'] == 101 and baseline['inputs-unchanged']
    assert baseline['inputs-before'] == baseline['inputs-after']
    for row in baseline['inputs-before']:
        bound(row, VALIDATION / 'baseline-source' / row['path'])
    output = text(baseline)
    assert '0 passed; 1 failed;' in output and 'selected NT directory parent type/name changed' in output
    focus = finished('session-alias-focus-epoch1.json')
    assert '3 passed; 0 failed;' in text(focus)
    cross = finished('session-alias-windows-cross-xwin-epoch2.json')
    assert cross['inputs-before'] == cross['inputs-after'] and cross['inputs-unchanged']
    assert [row['arch'] for row in cross['steps']] == ['aarch64', 'x86_64']
    for row in cross['steps']:
        assert row['exit-code'] == 0 and '--all-targets' in row['argv']
        text(row)
    native = native_capture()
    counts = {}
    for platform, passed in [('macos', 8102), ('linux', 8096), ('windows', 7564)]:
        gates = finished('gates-' + platform + '.json')
        chosen = steps(gates)
        expected = {'full', 'capi', 'integration', 'all-targets'}
        if platform == 'windows':
            expected |= {'toolchain', 'focus'}
        elif platform == 'linux':
            expected.add('focus')
        assert set(chosen) == expected
        for name, row in chosen.items():
            output = text(row)
            results = re.findall(r'test result: ([^\r\n]+)', output)
            assert row['results'] == results
            if name == 'full':
                failed = 5 if platform == 'windows' else 0
                assert f'{passed} passed; {failed} failed; 2 ignored; 0 measured; 0 filtered out;' in results[0]
                counts[platform] = {'passed': passed, 'failed': failed, 'ignored': 2}
                if failed:
                    assert row['exit-code'] == 101 and set(gates['exact-existing-failures']) == FAILURES
                    tail = output[output.rfind('\nfailures:\n'):]
                    assert set(re.findall(r'^    (\S+)\r?$', tail, re.M)) == FAILURES
                else:
                    assert row['exit-code'] == 0
            else:
                assert row['exit-code'] == 0
            expected_passes = {'focus': 4 if platform == 'windows' else 3, 'capi': 183, 'integration': 4 if platform == 'windows' else 544}
            if name in expected_passes:
                assert any(f'{expected_passes[name]} passed; 0 failed;' in result for result in results)
    owning = load(VALIDATION / 'session-alias-owning-source-hashes-epoch1.json')
    assert owning['epoch'] == 1 and len(owning['sources']) == 3826
    for row in owning['sources']:
        bound(row, source / row['path'])
    for row in owning['helpers']:
        bound(row, VALIDATION / row['path'])
    for platform in ['macos', 'linux', 'windows']:
        owner = finished('owning-' + platform + '.json')
        chosen = steps(owner)
        assert len(chosen) == (8 if platform == 'macos' else 5 if platform == 'linux' else 6)
        for row in chosen.values():
            assert row['exit-code'] == 0
            text(row)
        before = load(VALIDATION / ('archives-' + platform + '-before.json'))
        after = load(VALIDATION / ('archives-' + platform + '-after.json'))
        assert before['archives'] == after['archives']
        assert before['sources'] == after['sources'] == owning['sources']
        assert before['core-features'] == after['core-features'] == []
        assert len(before['archives']) == 2
        assert all(row['structural-member-walk'] == 'pass' and row['members'] > 0 for row in before['archives'])
        if platform == 'macos':
            assert len(re.findall(r'^\d+/7 Test .*\sPassed\s', text(chosen['ctest']), re.M)) == 7
            assert len(load(VALIDATION / 'macos-session-alias-epoch1-linkage.json')) == 6
    consumers(source)
    assumptions = load(VALIDATION / 'assumptions-final.json')['assumptions']
    assert all(row['status'] in ['confirmed', 'retained'] for row in assumptions)
    for row in assumptions:
        assert all(row.get(key) for key in ['id', 'assumption', 'basis', 'dependent-result', 'stress-test', 'falsification-probe'])
    summary = load(VALIDATION / 'summary.json')
    assert summary['library-gates'] == counts
    assert summary['ci-session-alias-target'] == 'unknown'
    assert summary['full-process-goal-complete'] is False
    assert summary['whoami-frontier'] == 'NtQueryAttributesFile0x3d'
    assert summary['independent-x64-kernel-validated'] is False
    assert summary['whole-current-root-products-validated'] is False
    quality = load(VALIDATION / 'quality-gates.json')
    assert set(quality['gates']) == {'QG' + str(index) for index in range(1, 10)}
    assert all(value.startswith('pass:') for value in quality['gates'].values())
    return {'host': host, 'state': 'verified', 'source-files': 25085, 'source-bytes': 1835122521,
            'owning-source-inputs': 3826, 'library-gates': counts, 'native': native,
            'full-process-goal-complete': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--host', required=True, choices=['macos', 'linux', 'windows'])
    parser.add_argument('--source', type=Path, default=SOURCE)
    args = parser.parse_args()
    print(json.dumps(check(args.host, args.source), indent=2))
