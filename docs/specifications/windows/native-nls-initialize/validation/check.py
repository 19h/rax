"""Portable verification of actual source epochs, test counts and owning proof."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).parent


def require(ok, message):
    if not ok:
        raise ValueError(message)


def read(name):
    return (ROOT / name).read_text(encoding='utf-8-sig')


def summaries(name):
    return [tuple(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;', read(name))]


def main():
    require(__debug__, 'optimized verification rejected')
    result = json.loads(read('results.json'))
    for row in result['files']:
        data = (ROOT / row['path']).read_bytes()
        require((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']), row['path'])
    epochs = {}
    for epoch in range(1, 5):
        rows = json.loads(read(f'epoch{epoch}-frozen-code.json'))['files']
        require(len(rows) == 14, 'fourteen selected execution inputs')
        epochs[epoch] = {row['path']: row['sha256'] for row in rows}
        for row in rows:
            data = (ROOT / f'compiled-source/epoch{epoch}' / row['path']).read_bytes()
            require((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']), 'compiled source ' + row['path'])
            if epoch == 4:
                require(hashlib.sha256((ROOT.parents[4] / row['path']).read_bytes()).hexdigest() == row['sha256'], 'published source ' + row['path'])
    require({name for name in epochs[3] if epochs[3][name] != epochs[4][name]} ==
            {'src/user/windows/nls/installed/initialize.rs'}, 'only Windows host adapter differs between final Unix full and final native snapshots')
    for host, count in [('macos', 7830), ('linux', 7824)]:
        require(summaries(f'{host}-epoch3/{host}-lib.log') == [(count, 0, 2, 0)], host + ' full library')
        require((168, 0, 0, 0) in summaries(f'{host}-epoch3/{host}-capi.log'), host + ' C API')
        require((544, 0, 0, 0) in summaries(f'{host}-epoch3/{host}-windows.log'), host + ' actual shared Windows integration')
        require(any(row[:3] == (13, 0, 0) for row in summaries(f'{host}-epoch4/{host}-focused.log')), host + ' final source focus')
        for phase, count in [('epoch3', 7), ('epoch4', 2)]:
            gates = json.loads(read(host + '-' + phase + '/gates.json'))
            require(len(gates) == count and all(row['exit-code'] == 0 for row in gates), host + phase + ' completed gates')
    native = 'windows/native-nls-initialize-epoch4-'
    require(summaries(native + 'full.log') == [(7253, 5, 2, 0)], 'native Windows full library')
    require(any(row[:3] == (14, 0, 0) for row in summaries(native + 'focused.log')), 'fourteen actual native/replay NLS tests')
    require((168, 0, 0, 0) in summaries(native + 'capi.log'), 'native C API')
    require((4, 0, 0, 0) in summaries(native + 'integration.log'), 'four actual native memory cases')
    text = read(native + 'focused.log')
    for arch in ['arm64', 'x86']:
        require(f'Validated installed {arch} NtInitializeNlsFiles ABI-specific arguments and locale.nls bytes with real unmap' in text, 'actual installed leaf ' + arch)
    text = read(native + 'full.log')
    observed = set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))
    expected = {
        'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
        'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
        'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
        'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
        'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime'}
    require(observed == expected, 'exact five unchanged lowerer failure identities')
    gates = json.loads(read(native + 'gates.json'))
    require(len(gates) == 6 and all(row['exit-code'] == (101 if row['gate'] == 'full' else 0) for row in gates), 'six native completed gates')
    manifest = json.loads(read('epoch4-owning-source-hashes.json'))
    require(len(manifest['sources']) == 299, '299 selected source identities')
    for name in ['macos-epoch4-owning-archive-identities.json', 'linux-epoch4-owning-archive-identities.json', native + 'owning-archive-hashes.json']:
        data = json.loads(read(name))
        require(data['sources'] == manifest['sources'] and data['core-features'] == [], 'archive source/feature binding ' + name)
        require(len(data['archives']) == 2 and all(row['members'] > 200 and row['structural-member-walk'] == 'pass' for row in data['archives']), 'complete two archive walks ' + name)
    for host in ['macos', 'linux']:
        require(all(row['exit-code'] == 0 for row in json.loads(read(host + '-owning-gates.json'))), host + ' owning gates')
    require('100% tests passed out of 7' in read('macos-owning-ctest.log'), 'seven macOS owning CTests')
    for name in ['linux-owning-cpp.log', 'windows/assist-native-nls-initialize-epoch4-cpp.log']:
        for marker in ['rust_archive_link', 'rax_process', 'process_tool', 'process_tool_unavailable', 'rax_abi_drift']:
            require(marker in read(name), 'actual scoped C++ consumer ' + name + marker)
    require(json.loads(read('windows/nls-initialize-owning-epoch4-atomic-v2-launch.json'))['exit-code'] == 0, 'completed native owning driver')
    require(any(row[:3] == (12, 1, 0) for row in summaries('native-nls-initialize-epoch2-focused.log')), 'retained original strict x86 stack failure')
    require(len(json.loads(read('epoch1-native-nls-initialize-epoch1-compile-errors.json'))) == 8, 'retained eight initial Windows diagnostics')
    ordinary = read(native + 'ordinary-0.log')
    require('terminal turn=2132074' in ordinary and 'NtCreateFile (0x55)' in ordinary, 'unmodified ordinary startup frontier')
    require(result['ordinary-windows-guest-exits'] == 0, 'no ordinary completion claim')
    print('Verified14finalinputs,three-hostlibrary/CAPI/focus,299owninginputs,installedARM64/x86success,retainedfailuresandordinaryfrontier')


if __name__ == '__main__':
    main()
