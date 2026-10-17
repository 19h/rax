"""Verify completed, byte-bound evidence; reject incomplete or optimized runs."""
from pathlib import Path
import hashlib
import json
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
RAX = ROOT.parents[4]
KNOWN_WINDOWS_FAILURES = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read(name):
    return (ROOT / name).read_text(encoding='utf-8-sig')


def load(name):
    return json.loads(read(name))


def summaries(name):
    return [tuple(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',
        read(name))]


def failure_names(name):
    text = read(name)
    return set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))


def main():
    require(__debug__, 'optimized verification rejected')
    subprocess.run([sys.executable, '-B', str(ROOT.parent / 'check.py')], check=True)
    result = load('results.json')
    require(result['final-code-epoch'] == 2 and result['selected-owning-input-count'] == 309,
            'selected final source epoch and input count')
    seen = set()
    for row in result['files']:
        name = row['path']
        require(not Path(name).is_absolute() and '..' not in Path(name).parts and name not in seen,
                'relative unique evidence path')
        seen.add(name)
        data = (ROOT / name).read_bytes()
        require((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']), name)

    epochs = []
    for epoch in [1, 2]:
        rows = load(f'epoch{epoch}-console-owner-code.json')['files']
        require(len(rows) == 13, 'thirteen selected Rust inputs')
        epochs.append({row['path']: row['sha256'] for row in rows})
        for row in rows:
            data = (ROOT / f'compiled-source/epoch{epoch}' / row['path']).read_bytes()
            require((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']),
                    'compiled input ' + row['path'])
            if epoch == 2:
                require(hashlib.sha256((RAX / row['path']).read_bytes()).hexdigest() == row['sha256'],
                        'published input ' + row['path'])
    require(set(epochs[0]) == set(epochs[1]), 'identical selected input paths')
    require({name for name in epochs[0] if epochs[0][name] != epochs[1][name]} == {
        'src/user/windows/dll/native/process_console.rs',
        'src/user/windows/process/sched/tests/services_tests/process_console_tests.rs'},
        'only WoW NULL priority and synthetic transition helper differ between epochs')

    expected_sp_failures = {
        'user::windows::process::sched::tests::services_tests::process_console_tests::native_console_owner_tagged_values_are_process_local_without_objects_or_entropy',
        'user::windows::process::sched::tests::services_tests::process_console_tests::native_console_owner_rejected_tags_leave_prior_state_unchanged_all_abis',
        'user::windows::process::sched::tests::services_tests::process_console_tests::replay::native_console_owner_replays_all_544_rows_and_136_repeat_sets_from_four_native_profiles',
    }
    for host, count in [('macos', 7836), ('linux', 7830)]:
        for epoch in [1, 2]:
            gates = load(f'{host}-epoch{epoch}/gates.json')
            require(len(gates) == 7, 'seven completed core gates ' + host)
            require(all(row['exit-code'] == (101 if epoch == 1 and row['gate'] in ['focused', 'lib'] else 0)
                        for row in gates), 'expected gate exits ' + host)
            for row in gates:
                data = (ROOT / f'{host}-epoch{epoch}' / row['log']).read_bytes()
                require(hashlib.sha256(data).hexdigest() == row['sha256'], 'core log binding')
        require(summaries(f'{host}-epoch2/{host}-lib.log') == [(count, 0, 2, 0)],
                'unfiltered final full library ' + host)
        require(any(row[:3] == (6, 0, 0) for row in summaries(f'{host}-epoch2/{host}-focused.log')),
                'six actual shared focused tests ' + host)
        require((168, 0, 0, 0) in summaries(f'{host}-epoch2/{host}-capi.log'), 'C API ' + host)
        require((544, 0, 0, 0) in summaries(f'{host}-epoch2/{host}-windows.log'), 'integration ' + host)
        require(failure_names(f'{host}-epoch1/{host}-lib.log') == expected_sp_failures,
                'retained exact initial synthetic SP failures ' + host)
        owning = load(host + '-owning-gates.json')
        require(len(owning) == (6 if host == 'macos' else 4) and all(row['exit-code'] == 0 for row in owning),
                'completed scoped owning gates ' + host)

    native = 'windows/native-console-owner-epoch2-'
    gates = load(native + 'gates.json')
    require(len(gates) == 6 and all(row['exit-code'] == (101 if row['gate'] == 'full' else 0) for row in gates),
            'six native final gate outcomes')
    require(any(row[:3] == (7, 0, 0) for row in summaries(native + 'focused.log')),
            'seven native tests including installed leaves')
    require((168, 0, 0, 0) in summaries(native + 'capi.log'), 'native C API')
    require((4, 0, 0, 0) in summaries(native + 'integration.log'), 'four actual native memory tests')
    require(summaries(native + 'full.log') == [(7260, 5, 2, 0)], 'native unfiltered full library')
    require(failure_names(native + 'full.log') == KNOWN_WINDOWS_FAILURES, 'exact existing native lowerer failures')
    require(load('windows/console-owner-epoch1-launch.json')['exit-code'] == 101,
            'initial native driver has terminal101; handoff ledger distinguishes its meaning')
    handoff_path = ROOT / 'windows/console-owner-native-handoff.json'
    superseded = False
    if handoff_path.exists():
        handoff = load('windows/console-owner-native-handoff.json')
        require(handoff.get('state') == 'finished' and handoff.get('exit-code') == 0 and
                handoff.get('final-core-exit-code') == 0 and handoff.get('final-owning-exit-code') == 0,
                'native handoff completed the corrected gates')
        superseded = handoff.get('epoch1-native-focused', '').startswith('not run;')
        if superseded:
            require(not (ROOT / 'windows/native-console-owner-epoch1-focused.log').exists(),
                    'superseded phase has no manufactured native focused result')
        else:
            require(handoff.get('epoch1-native-focused', '').startswith('ran;'),
                    'native initial focused execution recorded explicitly')
        continuation = load('windows/console-owner-native-continuation.json')
        source = (ROOT / 'windows/run-console-owner-native-handoff.py').read_bytes()
        require(hashlib.sha256(source).hexdigest() == continuation['handoff-source-sha256'],
                'actual native continuation loaded handoff source identity')
    if not superseded:
        require(failure_names('windows/native-console-owner-epoch1-focused.log') == expected_sp_failures,
                'retained exact initial native synthetic SP failures')
    require(load('windows/console-owner-epoch2-launch.json')['exit-code'] == 0, 'final native driver terminated')
    require(load('windows/console-owner-owning-epoch2-atomic-v2-launch.json')['exit-code'] == 0,
            'native owning driver terminated')

    manifest = load('epoch2-owning-source-hashes.json')
    require(len(manifest['sources']) == 309, 'selected owning source identities')
    for row in manifest['sources']:
        data = (RAX / row['path']).read_bytes()
        require((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']),
                'current owning input ' + row['path'])
    for name in ['macos-final-owning-archive-identities.json', 'linux-final-owning-archive-identities.json',
                 native + 'owning-archive-hashes.json']:
        record = load(name)
        require(record['sources'] == manifest['sources'] and record['core-features'] == [],
                'archive source and feature binding ' + name)
        require(len(record['archives']) == 2 and all(row['members'] > 200 and row['structural-member-walk'] == 'pass'
                                                   for row in record['archives']), 'complete archive walks ' + name)
    require('100% tests passed out of 7' in read('macos-owning-ctest.log'), 'seven selected macOS CTests')
    for name in ['linux-owning-cpp.log', 'windows/assist-native-console-owner-epoch2-cpp.log']:
        for marker in ['rust_archive_link', 'rax_process', 'process_tool', 'process_tool_unavailable', 'rax_abi_drift']:
            require(marker in read(name), 'actual C++ consumer ' + name + '/' + marker)
    boundary = load('owning-validation-boundary.json')
    require((boundary['frozen-assist-public-abi-minor'], boundary['current-assist-public-abi-minor'],
             boundary['rax-public-abi']) == (184, 186, '1.11.0'), 'explicit retained carrier boundary')
    for epoch in [1, 2]:
        rows = load(f'windows/native-console-owner-epoch{epoch}-ordinary-outcomes.json')
        require(len(rows) == 3, 'ordinary startup observation selection')
        for row in rows:
            if 'log' in row:
                require(hashlib.sha256((ROOT / 'windows' / row['log']).read_bytes()).hexdigest() == row['sha256'],
                        'ordinary log bytes')
    print('Verified final source epochs, three-host core gates, actual installed leaves, '
          '309 scoped owning inputs, complete archive identities and retained failures')


if __name__ == '__main__':
    main()
