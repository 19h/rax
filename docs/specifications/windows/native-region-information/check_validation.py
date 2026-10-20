"""Verify frozen sources, independent native observations and executed consumers.

Run with Python -B. Only .cargo/config.toml has a declared Windows adapter.
The package records an incomplete process-emulation goal, not a release claim.
"""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import os
import re

DOCS = Path(__file__).resolve().parent
VALIDATION = DOCS / 'validation'
NATIVE = DOCS / 'native'
SOURCE = DOCS.parents[3]
BASE = 'dbcea8afa4292b4f5afa620963fe6f81e9301cae'
NATIVE_CONFIG = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
VERSIONS = list(range(1, 9)) + list(range(10, 17))
PROFILES = ['arm64', 'x64', 'x86', 'x86-laa']
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
    assert identity(path) == (row['bytes'], row['sha256']), str(path)


def log(row, base=VALIDATION):
    path = base / row['log']
    bound(row, path)
    return path.read_text(errors='replace')


def finished(name, base=VALIDATION):
    value = load(base / name)
    assert value['state'] == 'finished' and value['exit-code'] == 0, name
    return value


def steps(value):
    result = {row.get('name', row.get('gate')): row for row in value['steps']}
    assert len(result) == len(value['steps'])
    return result


def fields(line):
    return dict(part.split('=', 1) for part in line.split() if '=' in part)


def native_capture():
    runtime = None
    for version in VERSIONS:
        prefix = 'native-region-query-v' + str(version)
        launch = finished(prefix + '-launch.json', NATIVE)
        assert launch['source-before'] == launch['source-after']
        bound(launch['source-before'], NATIVE / (prefix + '.cpp'))
        assert launch['runtime-modules-before'] == launch['runtime-modules-after']
        assert len(launch['runtime-modules-before']) == 6
        if runtime is None:
            runtime = launch['runtime-modules-before']
        assert runtime == launch['runtime-modules-before']
        assert len(launch['steps']) == 16
        selected = steps(launch)
        assert [row['profile'] for row in launch['outcomes']] == PROFILES
        for row in selected.values():
            assert row['exit-code'] == 0
            log(row, NATIVE)
        for outcome, machine in zip(launch['outcomes'], [0xAA64, 0x8664, 0x14C, 0x14C]):
            profile = outcome['profile']
            assert outcome['exit-code'] == 0
            assert outcome['executable-before'] == outcome['executable-after']
            before = json.loads(log(selected[profile + '-before'], NATIVE))
            assert before == json.loads(log(selected[profile + '-after'], NATIVE))
            assert before['machine'] == machine
            assert before['large-address-aware'] == (profile != 'x86')
            assert (before['bytes'], before['sha256']) == (
                outcome['executable-before']['bytes'], outcome['executable-before']['sha256'])
            lines = log(selected[profile], NATIVE).splitlines()
            width = 4 if profile.startswith('x86') else 8
            assert fields(lines[0])['profile'] == profile
            assert int(fields(lines[0])['pointer']) == width
            # Producers 1/2 predate the terminal marker; their successful exit,
            # bound complete output and reconstructed case sets are checked.
            if version >= 3:
                assert lines[-1].startswith('finished=1')
            if version == 10:
                ending = fields(lines[-1])
                assert ending['handle-count-before'] == ending['handle-count-after']
            if version == 16:
                matrix = [fields(line) for line in lines if line.startswith('matrix ')]
                assert len(matrix) == 132
                assert {(int(row['class']), int(row['length'])) for row in matrix} == {
                    (kind, length) for kind in [3, 7] for length in range(66)}
                for row in matrix:
                    kind, length = int(row['class']), int(row['length'])
                    minimum = (16 if kind == 3 else 20) if width == 4 else (24 if kind == 3 else 32)
                    assert row['exception'] == '00000000'
                    assert int(row['status'], 16) == (0 if length >= minimum else 0xC0000004)
                    raw = bytes.fromhex(row['bytes'])
                    copied = min(length // width * width, 28 if width == 4 else 48)
                    if length < minimum:
                        copied = 0
                    assert raw[copied:] == bytes([0xA5]) * (len(raw) - copied)
                    expected = copied if length >= minimum else (1 << (8 * width)) - 1
                    assert int(row['returned']) == expected
    rejected = load(NATIVE / 'rejected-region-query-v9-launch.json')
    assert rejected['state'] == 'failed' and rejected['outcomes'][0]['exit-code'] == 3
    for row in rejected['steps']:
        log(row, NATIVE)
    kernel = finished('kernel-profile.json')
    assert (kernel['major'], kernel['minor'], kernel['build'], kernel['native-processor-architecture']) == (10, 0, 29683, 12)
    return {'profiles': PROFILES, 'successful-versions': VERSIONS, 'kernel-build': 29683}


def replay_projection():
    """Reconstruct every compiled row and its precise mask from raw native logs."""
    rows = []
    inputs = []
    for profile in PROFILES:
        width = 4 if profile.startswith('x86') else 8
        for version in [1, 2, 16]:
            path = NATIVE / ('native-region-query-v' + str(version) + '-' + profile + '.log')
            size, digest = identity(path)
            inputs.append({'path': path.name, 'bytes': size, 'sha256': digest})
            for line in path.read_text().splitlines():
                group = line.split()[0] if line else ''
                if (version in [1, 16] and group != 'matrix') or (version == 2 and group not in ['boundary', 'rights']):
                    continue
                record = fields(line)
                if record['class'] not in ['3', '7']:
                    continue
                assert record['exception'] == '00000000'
                row = {'profile': profile, 'group': group, 'class': int(record['class']),
                       'length': int(record['length']), 'status': int(record['status'], 16),
                       'bytes': record['bytes'], 'masked-byte-indices': []}
                if group == 'matrix':
                    row.update({'target': int(record['target']), 'returned-value': int(record['returned'])})
                else:
                    row.update({'role': int(record['role']), 'guards': record['guards'],
                                'returned-bytes': record['returned']})
                offset = 128
                if group == 'boundary':
                    offset = {1: 132, 2: 130}.get(row['role'], 128)
                    if row['role'] in [3, 4, 14, 16, 17, 18, 27, 35, 36, 37]:
                        offset = 4096
                if row['status'] == 0 and offset < 192:
                    copied = min(row['length'] // width * width, 28 if width == 4 else 48)
                    starts = [0, width + 8, width * 2 + 8]
                    if group == 'boundary' and row['role'] == 22:
                        starts.remove(0)
                    row['masked-byte-indices'] = [
                        offset - 128 + index for start in starts
                        for index in range(start, min(start + width, copied))
                        if 0 <= offset - 128 + index < 64]
                rows.append(row)
    compiled = load(DOCS / 'modelled-query-cases.json')
    assert compiled['rows'] == rows and compiled['native-log-inputs'] == inputs
    assert len(rows) == 1608
    assert {group: sum(row['group'] == group for row in rows) for group in ['matrix', 'boundary', 'rights']} == {
        'matrix': 1256, 'boundary': 304, 'rights': 48}


def consumers(source):
    owning = load(VALIDATION / 'region-owning-source-hashes-epoch1.json')
    assert owning['epoch'] == 1 and len(owning['sources']) == 3823
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
            log(row)
        before = load(VALIDATION / ('archives-' + platform + '-before.json'))
        after = load(VALIDATION / ('archives-' + platform + '-after.json'))
        assert before['archives'] == after['archives']
        assert before['sources'] == after['sources'] == owning['sources']
        assert before['core-features'] == after['core-features'] == []
        assert len(before['archives']) == 2
        assert all(row['structural-member-walk'] == 'pass' and row['members'] > 0 for row in before['archives'])
        if platform == 'macos':
            assert len(re.findall(r'^\d+/7 Test .*\sPassed\s', log(selected['ctest']), re.M)) == 7
            assert len(load(VALIDATION / 'macos-region-epoch1-linkage.json')) == 6
    ordinary = finished('ordinary.json')
    capi = finished('capi.json')
    for receipt in [ordinary, capi]:
        for key in receipt:
            if key.endswith('-before'):
                assert receipt[key] == receipt[key[:-7] + '-after'], key
        for row in receipt['steps']:
            assert row['exit-code'] == 0
            log(row)
    windows = load(VALIDATION / 'archives-windows-after.json')
    identities = {(row['bytes'], row['sha256']) for row in windows['archives']}
    assert (ordinary['core-after']['bytes'], ordinary['core-after']['sha256']) in identities
    assert (capi['carrier-after']['bytes'], capi['carrier-after']['sha256']) in identities
    bound(ordinary['observer-after'], VALIDATION / 'region-ordinary-owner-epoch1.rs')
    bound(capi['source-after'], VALIDATION / 'region-owning-capi.cpp')
    chosen = steps(ordinary)
    smoke = log(chosen['smoke'])
    whoami = log(chosen['whoami'])
    assert 'Complete(Exited(0))' in smoke
    assert 'reached-continue=1 reached-regions=0 terminal=true' in smoke
    assert whoami.count('region-result ') == 12
    assert len(re.findall(r'^region-result .*status=0x0\b', whoami, re.M)) == 12
    assert 'NtQueryAttributesFile (0x3d)' in whoami and 'turn=7618121' in whoami
    assert 'reached-regions=12 terminal=true' in whoami
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
    envelope = load(DOCS / 'evidence-manifest.json')
    names = set()
    for row in envelope['files']:
        assert row['path'] not in names
        names.add(row['path'])
        bound(row, DOCS / row['path'])
    actual = {p.relative_to(DOCS).as_posix() for p in DOCS.rglob('*') if p.is_file()}
    assert actual == names | {'evidence-manifest.json'}
    frozen = load(VALIDATION / 'region-source-epoch1.json')
    assert frozen['base-repository-commit'] == BASE and frozen['source-epoch'] == 1
    assert len(frozen['files']) == 25082 and frozen['total-bytes'] == 1835096144
    assert sum(row['bytes'] for row in frozen['files']) == frozen['total-bytes']
    for row in frozen['files']:
        value = identity(source / row['path'])
        assert value == (row['bytes'], row['sha256']) or (
            host == 'windows' and row['path'] == '.cargo/config.toml' and value == (374, NATIVE_CONFIG)), row['path']
    baseline = load(VALIDATION / 'region-baseline.json')
    assert baseline['exit-code'] == 101 and baseline['inputs-unchanged']
    assert baseline['inputs-before'] == baseline['inputs-after']
    assert '0 passed; 2 failed;' in log(baseline)
    assert 'NtQueryVirtualMemory' in log(baseline)
    focus = finished('region-focus-epoch7.json')
    assert focus['inputs-before'] == focus['inputs-after']
    assert '19 passed; 0 failed;' in log(focus)
    native = native_capture()
    replay_projection()
    counts = {}
    for platform, passed in [('macos', 8099), ('linux', 8093), ('windows', 7560)]:
        gates = finished('gates-' + platform + '.json')
        selected = steps(gates)
        expected = {'full', 'capi', 'integration', 'all-targets', 'copy-focus'}
        if platform in ['windows', 'linux']:
            expected.add('focus')
        if platform == 'windows':
            expected |= {'toolchain', 'installed-leaves'}
        assert set(selected) == expected, (platform, set(selected))
        for name, row in selected.items():
            output = log(row)
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
            expected_passes = {'focus': 19, 'copy-focus': 6, 'installed-leaves': 1, 'capi': 183,
                               'integration': 4 if platform == 'windows' else 544}
            if name in expected_passes:
                assert any(f'{expected_passes[name]} passed; 0 failed;' in result for result in results)
    consumers(source)
    assumptions = load(VALIDATION / 'assumptions-final.json')['assumptions']
    assert len(assumptions) == 5
    for row in assumptions:
        assert row['status'] in ['confirmed', 'retained']
        assert all(row.get(key) for key in ['id', 'assumption', 'basis', 'dependent-result', 'stress-test', 'falsification-probe'])
    summary = load(VALIDATION / 'summary.json')
    assert summary['library-gates'] == counts
    assert summary['compiled-replay-observations'] == 1608
    assert summary['whoami-successful-region-queries'] == 12
    assert summary['whoami-frontier'] == 'NtQueryAttributesFile0x3d'
    for key in ['full-process-goal-complete', 'whole-current-root-products-validated', 'independent-x64-kernel-validated']:
        assert summary[key] is False
    quality = load(VALIDATION / 'quality-gates.json')
    assert set(quality['gates']) == {'QG' + str(index) for index in range(1, 10)}
    assert all(value.startswith('pass:') for value in quality['gates'].values())
    return {'host': host, 'state': 'verified', 'source-files': 25082, 'source-bytes': 1835096144,
            'owning-source-inputs': 3823, 'library-gates': counts, 'native': native,
            'full-process-goal-complete': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--host', required=True, choices=['macos', 'linux', 'windows'])
    parser.add_argument('--source', type=Path, default=SOURCE)
    arguments = parser.parse_args()
    print(json.dumps(check(arguments.host, arguments.source), indent=2))
