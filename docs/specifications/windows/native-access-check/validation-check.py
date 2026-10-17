"""Verify source epochs, actual tests and scoped embedding for this change."""
from pathlib import Path
import hashlib
import json
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
V = ROOT / 'validation'
RAX = ROOT.parents[3]
FAILURES = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}

def need(value, message):
    if not value:
        raise ValueError(message)

def text(name):
    return (V / name).read_text(encoding='utf-8-sig')

def load(name):
    return json.loads(text(name))

def summaries(name):
    return [tuple(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;', text(name))]

def checked_gates(name, expected):
    rows = load(name)
    need(len(rows) == len(expected) and {r['gate'] for r in rows} == set(expected), 'complete unique gates ' + name)
    for row in rows:
        need(row['exit-code'] == expected[row['gate']], 'gate status ' + name + '/' + row['gate'])
        if 'sha256' in row:
            path = ('windows/' if name.startswith('windows/') else '') + row['log']
            need(hashlib.sha256((V / path).read_bytes()).hexdigest() == row['sha256'], 'gate log identity ' + path)
    return rows

def main():
    need(__debug__, 'optimized verifier rejected')
    subprocess.run([sys.executable, '-B', str(ROOT / 'check.py')], check=True)
    manifest = load('results.json')
    need(manifest['parent'] == '23ac34c5d0d0f6afff762431442d2fab52cda7f9'
         and manifest['selected-owning-inputs'] == 319, 'published parent/source epoch')
    seen = set()
    for row in manifest['files']:
        name = row['path']
        need(name not in seen and not Path(name).is_absolute() and '..' not in Path(name).parts, 'unique relative evidence')
        seen.add(name)
        data = (V / name).read_bytes()
        need((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']), name)
    stress = load('access-validation-stress-results.json')
    need(len(stress) == 5 and {r['case'] for r in stress} == {
        'raw-corruption', 'empty-native-gates', 'forged-zero-failures',
        'incomplete-owning-driver', 'missing-final-unix-gates'}
        and all(r['rejected'] is True for r in stress), 'five rejected evidence corruptions')
    old = load('initial-overlay-identities.json')['files']
    final = load('final-source-inputs.json')['files']
    need(len(old) == len(final) == 17 and [r['path'] for r in old] == [r['path'] for r in final], 'seventeen explicit inputs')
    changed = [a['path'] for a, b in zip(old, final) if a['sha256'] != b['sha256']]
    need(changed == ['src/user/windows/process/sched/tests/services_tests/access_check_tests.rs'], 'only the test fixture changed')
    for label, rows in [('initial', old), ('final', final)]:
        for row in rows:
            data = (V / 'compiled-source' / label / row['path']).read_bytes()
            need((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']), 'compiled source ' + label)
    for host, full in [('macos', 7848), ('linux', 7842)]:
        checked_gates(host + '-access-initial-gates.json', {'access': 101})
        need(any(row[:3] == (3, 3, 0) for row in summaries(host + '-access-initial-access.log')), 'retain original fixture failures')
        checked_gates(host + '-access-epoch2-gates.json', {'access': 0, 'token': 0, 'wnf': 0})
        for suffix, count in [('access', 6), ('token', 3), ('wnf', 4)]:
            need(any(row[:3] == (count, 0, 0) for row in summaries(host + '-access-epoch2-' + suffix + '.log')), 'final focused ' + host + '/' + suffix)
        checked_gates(host + '-access-final-gates.json', {'all-targets': 0, 'windows': 0, 'capi': 0, 'lib': 0})
        need(summaries(host + '-access-final-lib.log') == [(full, 0, 2, 0)], 'complete unfiltered library ' + host)
        need((544, 0, 0, 0) in summaries(host + '-access-final-windows.log'), 'Windows integration ' + host)
        need((168, 0, 0, 0) in summaries(host + '-access-final-capi.log'), 'C API ' + host)
        expected = ({'build': 0, 'ctest': 0, 'protected-assist-cli': 0, 'protected-assist-mcp': 0, 'artifacts': 0, 'archive-identities': 0}
                    if host == 'macos' else {'shipping': 0, 'cpp': 0, 'artifacts': 0, 'archive-identities': 0})
        checked_gates(host + '-owning-gates.json', expected)
    checked_gates('windows/native-access-check-initial-gates.json', {'access': 101, 'token': 0, 'wnf': 0, 'all-targets': 0})
    need(any(row[:3] == (4, 3, 0) for row in summaries('windows/native-access-check-initial-access.log')), 'retain initial native fixture failures and installed pass')
    rows = load('windows/native-access-check-final-verified-gates.json')['gates']
    need(len(rows) == 7 and {r['gate'] for r in rows} == {'access', 'token', 'wnf', 'all-targets', 'integration', 'capi', 'full'}, 'complete native gates')
    for row in rows:
        need(row['exit-code'] == (101 if row['gate'] == 'full' else 0), 'native gate status')
        need(hashlib.sha256((V / 'windows' / row['log']).read_bytes()).hexdigest() == row['sha256'], 'native gate log')
    prefix = 'windows/native-access-check-final-'
    need(summaries(prefix + 'full.log') == [(7274, 5, 2, 0)], 'complete native library')
    raw = text(prefix + 'full.log')
    need(set(re.findall(r'^    (\S+)$', raw[raw.rfind('\nfailures:\n'):], re.M)) == FAILURES, 'exact pre-existing Windows failures')
    for suffix, count in [('access', 7), ('token', 4), ('wnf', 4)]:
        need(any(row[:3] == (count, 0, 0) for row in summaries(prefix + suffix + '.log')), 'native focused ' + suffix)
    need((168, 0, 0, 0) in summaries(prefix + 'capi.log'), 'native C API')
    need((4, 0, 0, 0) in summaries(prefix + 'integration.log'), 'native memory')
    driver = load('windows/access-check-final-launch.json')
    need(driver['state'] == 'finished' and driver['exit-code'] == driver['core-exit-code'] == driver['owning-exit-code'] == 0, 'complete native driver')
    sources = load('epoch2-owning-source-hashes.json')['sources']
    need(len(sources) == 319, '319 selected source inputs')
    for row in sources:
        data = (RAX / row['path']).read_bytes()
        need((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']), 'current source ' + row['path'])
    for name in ['macos-final-owning-archive-identities.json', 'linux-final-owning-archive-identities.json', prefix + 'owning-archive-hashes.json']:
        record = load(name)
        need(record['sources'] == sources and record['core-features'] == [], 'archive source/features ' + name)
        need(len(record['archives']) == 2 and all(r['members'] > 200 and r['structural-member-walk'] == 'pass' for r in record['archives']), 'two complete archives ' + name)
    need('100% tests passed out of 7' in text('macos-owning-ctest.log'), 'seven CTests')
    for name in ['linux-owning-cpp.log', 'windows/assist-native-access-check-final-cpp.log']:
        for marker in ['rust_archive_link', 'rax_process', 'process_tool', 'process_tool_unavailable', 'rax_abi_drift']:
            need(marker in text(name), 'C++ consumer ' + marker)
    boundary = load('owning-validation-boundary.json')
    need((boundary['frozen-assist-public-abi-minor'], boundary['current-assist-public-abi-minor'], boundary['rax-public-abi']) == (184, 186, '1.11.0'), 'retained carrier ABI scope')
    outcomes = load('windows/native-access-check-final-ordinary-outcomes.json')
    need(len(outcomes) == 3 and sum('log' in r for r in outcomes) == 2, 'two real observations and one explicit exclusion')
    for row in outcomes:
        if 'log' in row:
            need(hashlib.sha256((V / 'windows' / row['log']).read_bytes()).hexdigest() == row['sha256'], 'ordinary raw log')
    print('Verified access-check source epochs, three-host counts, installed leaves, 319 inputs and scoped complete archives')

if __name__ == '__main__':
    main()
