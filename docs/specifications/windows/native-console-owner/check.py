"""Check retained native identities, calls, snapshots and primary-source bytes."""
from pathlib import Path, PureWindowsPath
import hashlib
import json
import re

ROOT = Path(__file__).resolve().parent


def read_json(name):
    return json.loads((ROOT / name).read_text(encoding='utf-8-sig'))


def check_identity(name, row):
    data = (ROOT / name).read_bytes()
    assert len(data) == row['bytes'], name
    assert hashlib.sha256(data).hexdigest() == row['sha256'].lower(), name


def fields(line):
    return dict(part.split('=', 1) for part in line.split() if '=' in part)


def check():
    if not __debug__:
        raise RuntimeError('optimized validation is rejected')
    native = read_json('native-console-owner-identities.json')
    assert native['kernel'][:3] == [10, 0, 29683]
    rows = native['producers']
    assert len(rows) == 16
    assert {(r['family'], r['profile']) for r in rows} == {
        (family, profile)
        for family in ['matrix', 'initial', 'plain', 'redirected']
        for profile in ['arm64', 'x64', 'x86', 'x86-laa']
    }
    stems = {'matrix': 'native-console-owner', 'initial': 'native-console-host-initial',
             'plain': 'native-detached-process-parameters', 'redirected': 'native-detached-redirected-parameters'}
    primary_calls = repeats = successes = snapshots = 0
    for row in rows:
        profile = row['profile']
        width = 4 if profile.startswith('x86') else 8
        assert row['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile]
        if width == 4:
            assert bool(row['characteristics'] & 32) == (profile == 'x86-laa')
        source = PureWindowsPath(row['source']['path']).name
        stdout = PureWindowsPath(row['stdout']['path']).name
        check_identity(source, row['source'])
        check_identity(stdout, row['stdout'])
        outcomes = read_json(stems[row['family']] + '-outcomes.json')
        outcome = next(r for r in outcomes if r['profile'] == profile)
        assert outcome['exit-code'] == 0
        assert outcome['source-hash'].lower() == row['source']['sha256']
        assert outcome['exe-sha256'].lower() == row['executable']['sha256']
        assert outcome['exe-bytes'] == row['executable']['bytes']
        text = (ROOT / stdout).read_text()
        if row['family'] == 'matrix':
            results = re.findall(r'^result case=(\d+) status=(\w+) escaped=(\w+)', text, re.M)
            assert len(results) == 136
            assert {int(r[0]) for r in results} == set(range(136))
            assert all(int(r[2], 16) == 0 for r in results)
            assert text.splitlines()[-1] == f'complete cases=136 child-failures=0 width={width}'
            assert len(re.findall(r'^child-exit case=\d+ code=77$', text, re.M)) == 136
            repeated = re.findall(r'^second value=\w+ status=\w+ escaped=(\w+)', text, re.M)
            assert len(repeated) == 34 and all(int(r, 16) == 0 for r in repeated)
            helper_lines = [fields(line) for line in text.splitlines() if line.startswith('state phase=')]
            assert len(helper_lines) == 306
            assert all(int(line['escaped'], 16) == 0 for line in helper_lines)
            assert all(int(line['status'], 16) == (0xC0000003 if width == 4 else 0) for line in helper_lines)
            assert len(re.findall(r'^bytes name=buffer-after .* cells=[A-F0-9?]{32}$', text, re.M)) == 136
            assert len(re.findall(r'^bytes name=returned-after .* cells=[A-F0-9?]{8}$', text, re.M)) == 136
            primary_calls += len(results)
            repeats += len(repeated)
            successes += sum(int(r[1], 16) == 0 for r in results)
            assert row['primary-success'] == sum(int(r[1], 16) == 0 for r in results)
            if width == 4:
                for block in text.split('child-exit case='):
                    case = next((line for line in block.splitlines() if line.startswith('case=')), None)
                    if case and fields(case)['setting'] == '0':
                        result = next(line for line in block.splitlines() if line.startswith('result case='))
                        assert fields(result)['status'] == 'C0000003'
            leaves = row['loaded-leaves']
            assert {leaf['name'] for leaf in leaves} == {'NtSetInformationProcess', 'NtQueryInformationProcess'}
            assert all(int(leaf['pointer-bytes']) == width and len(leaf['hex']) == 64 for leaf in leaves)
        else:
            capture = re.search(r'^parameters bytes=(\d+) hex=(\w+)', text, re.M)
            raw = bytes.fromhex(capture[2])
            assert len(raw) == int(capture[1]) == (708 if width == 4 else 1096)
            assert int.from_bytes(raw[16:16 + width], 'little') == (1 << (8 * width)) - 1
            offset = 0x68 if width == 4 else 0xA4
            assert int.from_bytes(raw[offset:offset + 4], 'little') == (0 if row['family'] == 'plain' else 0x100)
            assert 'cleanup terminate=1 error=0 wait=00000000' in text
            if row['family'] != 'plain':
                assert text.splitlines()[-1] == 'cleanup-streams input=1 output=1 error=1'
            if row['family'] == 'initial':
                creation = fields(next(line for line in text.splitlines() if line.startswith('creation-owner ')))
                owner = fields(next(line for line in text.splitlines() if line.startswith('suspended-owner ')))
                assert int(creation['parent-pid']) == int(creation['basic-parent'], 16)
                assert int(creation['expected'], 16) == (int(creation['parent-pid']) | 2)
                if width == 8:
                    assert int(owner['status'], 16) == 0
                    assert int(owner['value'], 16) == int(creation['expected'], 16)
                else:
                    assert int(owner['status'], 16) == 0xC0000003
            snapshots += 1
    assert (primary_calls, repeats, successes, snapshots) == (544, 136, 130, 12)
    for reference in read_json('primary-sources.json'):
        check_identity(reference['file'], reference)
    # The negative baseline and controlled persona experiment are distinct.
    frontier = (ROOT / 'native-create-file-frontier.log').read_text()
    name = next(line.split('UTF16-name ', 1)[1] for line in frontier.splitlines() if 'UTF16-name ' in line)
    assert json.loads(name) == '\\Device\\ConDrv\\Server'
    controlled = (ROOT / 'native-console-host-0.log').read_text()
    assert 'controlled-detached before console=Ok(0)' in controlled
    assert 'class=49' in controlled and 'bytes=Ok([01, 00, 00, 00, 00, 00, 00, 00])' in controlled
    assert 'NtSetInformationProcess class 49' in controlled
    return {'verified-native-producers': 16, 'primary-calls': primary_calls,
            'primary-success': successes, 'primary-failure': primary_calls - successes,
            'repeat-sets': repeats, 'state-helper-queries': 1224, 'seh': 0,
            'suspended-parameter-snapshots': snapshots,
            'scope': 'retained independent native records and bytes; this portable check does not rerun native producers or prove ordinary guest completion'}


if __name__ == '__main__':
    print(json.dumps(check(), indent=2))
