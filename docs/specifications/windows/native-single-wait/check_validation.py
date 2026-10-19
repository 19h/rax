"""Verify retained native-wait evidence and its exact compiled source snapshot."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import sys

# The retained Windows embeddable Python omits the script directory from its
# isolated module search path. Resolve the adjacent checker explicitly.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from check_native import check_native, load, need

BASE_FAILURES = sorted([
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
])

def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            value.update(chunk)
    return value.hexdigest()

def bound_files(root):
    manifest = load(root / 'evidence.json')
    names = [row['path'] for row in manifest['files']]
    need(names == sorted(set(names)), 'duplicate or unsorted evidence paths')
    actual = sorted(path.relative_to(root).as_posix() for path in root.rglob('*')
        if path.is_file() and path.name != 'evidence.json' and '__pycache__' not in path.parts)
    need(names == actual, 'unbound or missing evidence files')
    for row in manifest['files']:
        path = root / row['path']
        need(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'], 'evidence drift: ' + row['path'])
    return len(names)

def source_inputs(root, source):
    manifest = load(root / 'validation/wait-source-epoch4.json')
    need(manifest['base'] == '889bc914e298cb58d8ab0b90c9181ef75083f97c' and manifest['source-epoch'] == 4, 'source epoch/base')
    need(len(manifest['files']) == 22059 and sum(row['bytes'] for row in manifest['files']) == 1510895073, 'complete snapshot counts')
    need(sum(row['kind'] == 'symlink-text' for row in manifest['files']) == 16, 'portable link representations')
    updates = load(root / 'validation/engineering-source-updates.json')
    need([row['path'] for row in updates['files']] == ['src/user/windows/native-runtime.md'], 'unexpected post-build source update')
    updates = {row['path']: row for row in updates['files']}
    for row in manifest['files']:
        path = source / row['path']
        if row['kind'] == 'symlink-text' and path.is_symlink():
            raw = os.readlink(path).encode()
        else:
            raw = path.read_bytes()
        expected = row['sha256']
        size = row['bytes']
        if row['path'] in updates:
            update = updates[row['path']]
            need(update['compiled-snapshot-sha256'] == expected, 'engineering baseline identity')
            # Native validation keeps the complete original snapshot. Repository
            # publication adds only this noncompiled engineering record afterward.
            if hashlib.sha256(raw).hexdigest() != expected:
                expected, size = update['final-sha256'], update['final-bytes']
        if row['path'] == '.cargo/config.toml' and os.name == 'nt':
            expected, size = '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c', 374
        need(len(raw) == size and hashlib.sha256(raw).hexdigest() == expected, 'compiled source drift: ' + row['path'])
    return len(manifest['files'])

def results(path):
    return re.findall(r'test result: ([^\r\n]+)', path.read_text())

def platform_gates(root):
    directory = root / 'validation'
    summary = {}
    for host, record, focus_count, passed in [
        ('macos', 'wait-macos-epoch4-gates.json', 16, 7961),
        ('linux', 'wait-linux-epoch4-gates.json', 16, 7955),
        ('windows', 'wait-native-epoch4-gates.json', 17, 7401),
    ]:
        data = load(directory / record)
        need(data['state'] == 'finished' and data['exit-code'] == 0, host + ' gates incomplete')
        steps = {row['name']: row for row in data['steps']}
        wanted = ['focus', 'full', 'capi', 'integration', 'all-targets']
        need(set(steps) == set(wanted + (['toolchain'] if host == 'windows' else [])), host + ' gate set')
        for name, row in steps.items():
            log = directory / row['log']
            need(digest(log) == row['sha256'], host + ' ' + name + ' log identity')
            need(results(log) == row['results'], host + ' ' + name + ' result projection')
            need('--locked' in row['argv'] and '--offline' in row['argv'] and '--no-default-features' in row['argv'] or name == 'toolchain', host + ' build flags')
            need(row['exit-code'] == (101 if host == 'windows' and name == 'full' else 0), host + ' ' + name + ' exit')
        need(any(row.startswith('ok. ' + str(focus_count) + ' passed; 0 failed; 0 ignored;') for row in steps['focus']['results']), host + ' focused coverage')
        full = steps['full']['results']
        need(len(full) == 1, host + ' full result count')
        expected = ('FAILED. ' if host == 'windows' else 'ok. ') + str(passed) + ' passed; ' + ('5' if host == 'windows' else '0') + ' failed; 2 ignored; 0 measured; 0 filtered out;'
        need(full[0].startswith(expected), host + ' full counts')
        if host == 'windows':
            need(data['exact-existing-failures'] == BASE_FAILURES, 'Windows exact existing failure set')
            text = (directory / steps['full']['log']).read_text()
            failures = sorted(re.findall(r'^    (\S+)\r?$', text[text.rfind('\nfailures:\n'):], re.M))
            need(failures == BASE_FAILURES, 'Windows failure trace projection')
            need(data['prepared-source-inputs'] == 22059 and data['post-source-inputs'] == 22059, 'Windows complete closure guards')
        need(any(row.startswith('ok. 168 passed; 0 failed;') for row in steps['capi']['results']), host + ' C API coverage')
        integration = steps['integration']['results']
        need(len(integration) == 2, host + ' integration binary count')
        counts = [0, 4] if host == 'windows' else [544, 0]
        need(all(row.startswith('ok. ' + str(count) + ' passed; 0 failed;') for row, count in zip(integration, counts)), host + ' integration coverage')
        summary[host] = {'focus': focus_count, 'full-passed': passed, 'full-failed': 5 if host == 'windows' else 0, 'ignored': 2}
    return summary

def consumer_gates(root):
    directory = root / 'validation'
    selected = load(directory / 'wait-owning-source-hashes-epoch4.json')
    need(len(selected['sources']) == 474 and selected['epoch'] == 'native-wait-epoch4', 'selected consumer source scope')
    data = load(directory / 'consumer-summary.json')
    need(data['frozen-rust-abi-minor'] == 184 and data['compared-root-rust-abi-minor'] == 264, 'carrier/current-root ABI boundary')
    need(data['compared-root-head'] == '8f49854be133693b69f546748fe207b23c6a23cd', 'compared root snapshot')
    need(data['selected-cpp-inputs'] == 8 and data['whole-current-root-product-validated'] is False, 'consumer claim boundary')
    for host, name in [('macos', 'macos-wait-epoch4-final-owning-archive-identities.json'),
        ('linux', 'linux-wait-epoch4-final-owning-archive-identities.json'),
        ('windows', 'native-wait-epoch4-final-owning-archive-identities.json')]:
        record = load(directory / name)
        need(record['host'] == host and record['core-features'] == [], host + ' carrier feature set')
        need(record['sources'] == selected['sources'], host + ' selected source binding')
        need(record['source-epoch'] == 'native-wait-epoch4', host + ' archive source epoch')
        need(len(record['archives']) == 2, host + ' two actual Cargo archives')
        need(record['core-artifact']['features'] == [], host + ' core artifact features')
        need(record['carrier-artifact']['target']['name'] == 'assist_rs', host + ' actual carrier artifact')
        for archive in record['archives']:
            need(archive['structural-member-walk'] == 'pass' and archive['members'] > 1 and archive['bytes'] > 1000000, host + ' complete archive walk')
            need(bool(re.fullmatch('[0-9a-f]{64}', archive['sha256'])), host + ' archive digest')
        need(all(row['exit-code'] == 0 for row in data['hosts'][host]['gates']), host + ' consumer gate failures')
        need(data['hosts'][host]['archives-changed-from-parent'] is True, host + ' stale parent archives')
        if host == 'windows':
            log = (directory / 'wait-native-owning-epoch4-cpp.log').read_text()
            before = [json.loads(line.split(' ', 1)[1]) for line in log.splitlines() if line.startswith('linked-archive-before ')]
            after = [json.loads(line.split(' ', 1)[1]) for line in log.splitlines() if line.startswith('linked-archive-after ')]
            need(len(before) == 1 and before == after, 'Windows C++ actual carrier before/after identity')
            carrier = next(row for row in record['archives'] if row['path'].endswith('.lib'))
            need(before[0]['sha256'] == carrier['sha256'] and before[0]['bytes'] == carrier['bytes'], 'Windows C++ linked a stale carrier')
            need(before[0]['path'].replace('\\', '/').lower() == carrier['path'].replace('\\', '/').lower(), 'Windows C++ carrier path')
    return data

def baseline_and_controls(root):
    directory = root / 'validation'
    need(any(row.startswith('FAILED. 0 passed; 9 failed; 0 ignored;') for row in results(directory / 'wait-baseline-v2.log')), 'corrected behavioral baseline')
    need(any(row.startswith('ok. 14 passed; 0 failed;') for row in results(directory / 'wait-prototype-epoch2.log')), 'corrected prototype coverage')
    need(any(row.startswith('FAILED. 0 passed; 1 failed; 0 ignored;') for row in results(directory / 'wait-ordinal-baseline.log')), 'ordinal compatibility baseline')
    need('existing ordinals or trap RVAs changed' in (directory / 'wait-ordinal-baseline.log').read_text(), 'ordinal baseline failure cause')
    need('called `Option::unwrap()` on a `None` value' in (directory / 'wait-baseline.log').read_text(), 'original fixture failure retained')
    stress = load(directory / 'wait-native-stress.json')
    need(len(stress['cases']) == 13 and all(row['result'] == 'rejected' for row in stress['cases']), 'native rejection controls')
    for revision, subdirectory in [('baseline-tests.json', 'baseline-tests'), ('baseline-tests-v2.json', 'baseline-tests-v2')]:
        record = load(directory / revision)
        need(len(record['files']) == 4, 'tests-only baseline source count')
        for row in record['files']:
            path = directory / subdirectory / row['path']
            need(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'], 'baseline test identity: ' + row['path'])

def ordinary_programs(root):
    directory = root / 'validation'
    record = load(directory / 'ordinary-program-summary.json')
    launch = load(directory / 'wait-owner-epoch4-launch.json')
    need(launch['state'] == 'finished' and launch['exit_code'] == 0, 'ordinary observer incomplete')
    need(launch['source-before'] == launch['source-after'], 'observer source changed')
    need(launch['executable-before'] == launch['executable-after'], 'observer executable changed')
    source_hash = '1dc6daedff021608ae91ea85270b498c130e9fed26177003629502fcd51bc227'
    need(launch['source-before']['sha256'] == source_hash and digest(directory / 'wait-owner-epoch4.rs') == source_hash, 'observer source identity')
    need(launch['runtime-modules-before'] == launch['runtime-modules-after'] and len(launch['runtime-modules-before']) == 6, 'ordinary runtime drift')
    modules = load(root / 'native-v5/native-wait-single-v5-modules-before.json')
    need([row['sha256'] for row in launch['runtime-modules-before']] == [row['Hash'].lower() for row in modules], 'ordinary runtime differs from native reference')
    owning = load(directory / 'native-wait-epoch4-final-owning-archive-identities.json')
    core = next(row for row in owning['archives'] if row['path'].endswith('.rlib'))
    need(launch['archive'] == core and launch['archive-after']['sha256'] == core['sha256'], 'ordinary actual core archive drift')
    need(len(launch['outcomes']) == 2, 'ordinary program outcome count')
    expected_programs = ['325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
        '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6']
    for row, expected in zip(launch['outcomes'], expected_programs):
        need(row['exit-code'] == 0 and row['program-before'] == row['program-after'], 'ordinary program/controller drift')
        need(row['program-before']['sha256'] == expected, 'ordinary program bytes changed')
        path = directory / row['log']
        need(digest(path) == row['log-identity']['sha256'], 'ordinary raw log identity')
    need(record['programs'] == 2 and record['guest-pc-or-status-patched'] is False and record['syscalls-suppressed'] is False, 'ordinary execution scope')
    need(record['actual-owner-core-bound'] is True, 'ordinary owner archive binding')
    need(record['full-process-goal-complete'] is False, 'overall goal qualification')
    # Concrete observed results and frontier text are additionally bound to the
    # raw driver logs; the summary cannot substitute for that replay evidence.
    for row in record['runs']:
        text = (directory / row['log']).read_text()
        need(row['native-wait-result-line'] in text, 'ordinary native wait result missing')
        need('status=Some(0)' in row['native-wait-result-line'], 'ordinary wait did not complete successfully')
        need(row['terminal-line'] in text and row['terminal-line'].startswith('terminal turn='), 'ordinary terminal evidence')
    return record

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source-root', type=Path, default=Path(__file__).resolve().parents[4])
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    bound = bound_files(root)
    native = check_native(root)
    compiled = source_inputs(root, args.source_root)
    platforms = platform_gates(root)
    consumer_gates(root)
    baseline_and_controls(root)
    ordinary = ordinary_programs(root)
    print(json.dumps({'status': 'passed', 'bound-evidence-files': bound, 'native-runs': len(native),
        'compiled-snapshot-inputs': compiled, 'platforms': platforms, 'ordinary-programs': ordinary['programs'],
        'full-process-goal-complete': False}))

if __name__ == '__main__':
    main()
