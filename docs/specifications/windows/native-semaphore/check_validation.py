"""Verify native semaphore evidence, frozen sources, platform gates and consumers."""
from pathlib import Path
import hashlib
import json
import os
import re
import struct
import sys


sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from check_native import check as check_native, load, need


BASELINE = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def bound(root):
    manifest = load(root / 'evidence.json')
    for row in manifest['files']:
        raw = (root / row['path']).read_bytes()
        need(len(raw) == row['bytes'] and digest(raw) == row['sha256'], 'bound evidence ' + row['path'])
    return len(manifest['files'])


def log(root, row):
    path = root / row['log']
    raw = path.read_bytes()
    need(digest(raw) == row['sha256'], 'log identity ' + row['log'])
    if 'bytes' in row:
        need(len(raw) == row['bytes'], 'log length ' + row['log'])
    text = raw.decode('utf-8', 'replace')
    if 'results' in row:
        need(re.findall(r'test result: ([^\r\n]+)', text) == row['results'], 'reported test results')
    return text


def source(root):
    repository = root.parents[3]
    manifest = load(root / 'validation/semaphore-source-epoch1.json')
    need(manifest['source-epoch'] == 1 and len(manifest['files']) == 22581, 'complete snapshot count')
    need(sum(row['bytes'] for row in manifest['files']) == 1566622159, 'complete snapshot bytes')
    updates = load(root / 'validation/engineering-source-updates.json')
    allowed = {row['path']: row for row in updates}
    lineage = load(root / 'validation/semaphore-integration-lineage.json')
    incoming = {row['path']: row for row in lineage['incoming-files']}
    need(len(incoming) == 11 and all(path.startswith('capi/') for path in incoming), 'bounded upstream C API changes')
    need(lineage['core-and-fixture-inputs-identical'] and lineage['core-has-no-capi-source-references'] and
         not lineage['root-build-script-exists'], 'unchanged core input contract')
    need(set(allowed) == {'src/user/windows/native-runtime.md'}, 'only engineering document may change after gates')
    for row in manifest['files']:
        path = repository / row['path']
        raw = os.readlink(path).encode() if row['kind'] == 'symlink-text' and path.is_symlink() else path.read_bytes()
        actual = digest(raw)
        if os.name == 'nt' and row['path'] == '.cargo/config.toml':
            need(actual == '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c' and len(raw) == 374, 'native Cargo source adapter')
            continue
        if row['path'] in incoming:
            change = incoming[row['path']]
            need(change['old-sha256'] == row['sha256'] and actual == change['new-sha256'], 'upstream C API input binding')
        elif actual != row['sha256']:
            update = allowed.get(row['path'])
            need(update is not None and update['compiled-sha256'] == row['sha256'] and
                 update['final-sha256'] == actual, 'compiled input drift ' + row['path'])
        else:
            need(len(raw) == row['bytes'], 'compiled input length ' + row['path'])
    integrated = load(root / 'validation/semaphore-source-epoch2.json')
    need(integrated['source-epoch'] == 2 and len(integrated['files']) == 22587, 'integrated full input count')
    need(sum(row['bytes'] for row in integrated['files']) == 1566675854, 'integrated full input bytes')
    old = {row['path']: row for row in manifest['files']}
    for row in integrated['files']:
        if row['path'] not in incoming:
            need(row == old.get(row['path']), 'non-CAPI integration changed a validated input')
        path = repository / row['path']
        raw = os.readlink(path).encode() if row['kind'] == 'symlink-text' and path.is_symlink() else path.read_bytes()
        if os.name == 'nt' and row['path'] == '.cargo/config.toml':
            need(digest(raw) == '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c' and len(raw) == 374, 'integrated native Cargo source adapter')
            continue
        update = allowed.get(row['path'])
        need(digest(raw) == row['sha256'] or update is not None and
             update['compiled-sha256'] == row['sha256'] and update['final-sha256'] == digest(raw), 'integrated source drift')
    selected = load(root / 'validation/semaphore-owning-source-hashes-epoch2.json')
    need(len(selected['sources']) == 506, 'selected owning input count')
    for row in selected['sources']:
        raw = (repository / row['path']).read_bytes()
        if digest(raw) != row['sha256']:
            update = allowed.get(row['path'])
            need(update is not None and update['compiled-sha256'] == row['sha256'] and
                 update['final-sha256'] == digest(raw), 'owning source drift ' + row['path'])
    preparation = load(root / 'validation/semaphore-native-epoch1-preparation.json')
    need(preparation['complete-inputs'] == 22581 and len(preparation['separate-overrides']) == 530,
         'native complete source materialization')
    need(preparation['deleted'] == ['src/user/windows/dll/native/mutant/attributes.rs'], 'old module deletion')
    need(preparation['immutable-linked-inputs'] + len(preparation['fallback-copies']) + 530 == 22581,
         'native materialization accounting')


def gates(root):
    report = {}
    for host, expected, filename in (
            ('macos', (7970, 0), 'semaphore-macos-epoch1-gates.json'),
            ('linux', (7964, 0), 'semaphore-linux-epoch1-gates.json'),
            ('windows', (7410, 5), 'semaphore-native-epoch1-gates.json')):
        record = load(root / 'validation' / filename)
        need(record['state'] == 'finished' and record['exit-code'] == 0, host + ' final gates incomplete')
        need(record['source-epoch'] == 1, host + ' source epoch')
        steps = {row['name']: row for row in record['steps']}
        need(set(steps) == ({'toolchain', 'focus', 'full', 'capi', 'integration', 'all-targets'}
                           if host == 'windows' else {'focus', 'full', 'capi', 'integration', 'all-targets'}),
             host + ' gate set')
        for name, row in steps.items():
            text = log(root / 'validation', row)
            need(row['exit-code'] == (101 if host == 'windows' and name == 'full' else 0), host + ' gate exit ' + name)
            if name == 'focus':
                need('9 passed; 0 failed; 0 ignored;' in text, host + ' exact9 focus selection')
            elif name == 'full':
                need(f'{expected[0]} passed; {expected[1]} failed; 2 ignored; 0 measured; 0 filtered out;' in text,
                     host + ' unfiltered full counts')
                if host == 'windows':
                    final = text[text.rfind('\nfailures:\n'):]
                    need(set(re.findall(r'^    (\S+)\r?$', final, re.M)) == BASELINE,
                         'exact five existing Windows failures')
            elif name == 'capi':
                need('168 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;' in text, host + ' C API selection')
            elif name == 'integration':
                need(('4 passed;' if host == 'windows' else '544 passed;') in text, host + ' integration selection')
        report[host] = {'passed': expected[0], 'failed': expected[1], 'ignored': 2}
    for host, filename in (('macos', 'semaphore-macos-epoch2-gates.json'),
                           ('linux', 'semaphore-linux-epoch2-gates.json'),
                           ('windows', 'semaphore-native-epoch2-gates.json')):
        record = load(root / 'validation' / filename)
        need(record['state'] == 'finished' and record['exit-code'] == 0 and record['source-epoch'] == 2, host + ' integration gates')
        steps = {row['name']: row for row in record['steps']}
        need(set(steps) == ({'toolchain', 'capi', 'all-targets'} if host == 'windows' else {'capi', 'all-targets'}), host + ' integrated gate set')
        for name, row in steps.items():
            need(row['exit-code'] == 0, host + ' integrated gate failure')
            text = log(root / 'validation', row)
            if name == 'capi':
                need('183 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;' in text, host + ' integrated C API selection')
    return report


def consumers(root):
    v = root / 'validation'
    summary = load(v / 'consumer-summary.json')
    need(summary['frozen-rust-abi-minor'] == 184 and not summary['whole-current-root-product-validated'],
         'retained carrier qualification')
    for host in ('macos', 'linux', 'windows'):
        if host == 'windows':
            name = 'semaphore-native-owning-epoch2.json'
            before_name = 'native-semaphore-epoch2-pre-cpp-archives.json'
            after_name = 'native-semaphore-epoch2-final-owning-archive-identities.json'
        else:
            name = host + '-semaphore-epoch2-owning-gates.json'
            before_name = host + '-semaphore-epoch2-pre-cpp-archive-identities.json'
            after_name = host + '-semaphore-epoch2-final-owning-archive-identities.json'
        record = load(v / name)
        need(record['state'] == 'finished' and record['exit-code'] == 0, host + ' consumers incomplete')
        for row in record['steps']:
            need(row['exit-code'] == 0, host + ' consumer failure')
            log(v, row)
        before, after = load(v / before_name), load(v / after_name)
        need(before['core-features'] == after['core-features'] == [], host + ' disabled core defaults')
        need(len(after['sources']) == 506 and before['archives'] == after['archives'], host + ' artifact lineage')
        need(len(after['archives']) == 2, host + ' actual core/carrier count')
        for row in after['archives']:
            need(row['members'] > 1 and row['bytes'] > 0 and row['structural-member-walk'] == 'pass', host + ' archive structure')
        if host in ('linux', 'windows'):
            cpp = next(row for row in record['steps'] if row.get('name', row.get('gate')) == 'cpp')
            text = log(v, cpp)
            linked = [json.loads(line.split(' ', 1)[1]) for line in text.splitlines()
                      if line.startswith(('linked-archive-before ', 'linked-archive-after '))]
            need(len(linked) == 2 and linked[0] == linked[1], host + ' C++ before/after carrier')
            carrier = next(row for row in after['archives'] if row['path'].endswith(('.lib', '.a')))
            need(all(linked[0][key] == carrier[key] for key in ('path', 'bytes', 'sha256')), host + ' stale C++ archive')
        else:
            links = load(v / 'macos-semaphore-epoch2-linkage.json')
            carrier = next(row for row in after['archives'] if row['path'].endswith('.a'))
            need(len(links) == 6 and all(row['carrier'] == carrier['path'] for row in links), 'actual macOS linker commands')
            ctest = next(row for row in record['steps'] if row['gate'] == 'ctest')
            text = log(v, ctest)
            passes = re.findall(r'^(\d+)/7 Test #\d+:\s+(\S+)\s+\.{2,}\s+Passed\s+', text, re.M)
            need([int(index) for index, _ in passes] == list(range(1, 8)), 'CTest progress indices')
            names = [name for _, name in passes]
            need(len(names) == 7 and set(names) == {
                'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
                'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
                'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test'}, 'seven actual macOS CTests')
            need(re.search(r'100% tests passed(?:, 0 tests failed)? out of 7', text) is not None, 'macOS CTest summary')


def ordinary(root):
    v = root / 'validation'
    record = load(v / 'semaphore-owner-epoch1-launch.json')
    need(record['state'] == 'finished' and record['exit_code'] == 0, 'ordinary observer incomplete')
    need(record['source-before'] == record['source-after'] and record['executable-before'] == record['executable-after'],
         'observer source/executable changed')
    need(digest((v / 'semaphore-owner-epoch1.rs').read_bytes()) == record['source-before']['sha256'], 'observer source identity')
    core = next(row for row in load(v / 'native-semaphore-epoch2-final-owning-archive-identities.json')['archives']
                if row['path'].endswith('.rlib'))
    need(record['archive'] == core and record['archive-after']['sha256'] == core['sha256'], 'observer used a different core')
    need(record['runtime-modules-before'] == record['runtime-modules-after'] and
         len(record['runtime-modules-before']) == 6, 'installed runtime drift')
    summary = load(v / 'ordinary-program-summary.json')
    need(len(summary['programs']) == len(record['outcomes']) == 2, 'both ordinary programs required')
    need(not any(row['guest-exit-success'] for row in summary['programs']), 'fabricated successful guest exit')
    for row, observed, expected_hash, advance in zip(summary['programs'], record['outcomes'],
            ('325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b',
             '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6'), (27400, 27336)):
        need(observed['exit-code'] == 0 and observed['program-before'] == observed['program-after'], 'ordinary program changed')
        need(observed['program-before']['sha256'] == expected_hash and row['program'] == observed['program-before'], 'program identity')
        raw = (v / row['log']).read_bytes()
        need(digest(raw) == row['sha256'] == observed['log-identity']['sha256'], 'ordinary raw log identity')
        text = raw.decode()
        need('prefix-turns=2100000 result=BudgetExhausted' in text, 'prefix was not fully executed')
        need('name=NtOpenSemaphore result=BudgetExhausted return-register=Some(c0000034)' in text, 'ordinary semaphore open result')
        need(text.count('name=NtCreateSemaphore result=BudgetExhausted return-register=Some(0)') == 2, 'ordinary semaphore creation')
        need(row['new-frontier-turn'] - row['prior-frontier-turn'] == row['advance-observer-turns'] == advance, 'observer turn arithmetic')
        need('NtOpenKey' in row['terminal'] and 'registry key outside selected runtime snapshot' in text,
             'actual remaining ordinary frontier')
    svc = load(v / 'native-semaphore-svc-map.json')
    need(svc == record['svc-map'], 'runtime SVC diagnostic binding')
    for row in svc['rows']:
        word = struct.unpack_from('<I', bytes.fromhex(row['first-16-bytes']), row['svc-offset'])[0]
        need(word & 0xFFE0001F == 0xD4000001 and (word >> 5) & 0xFFFF == row['svc'], 'captured SVC bytes')


def check(root):
    count = bound(root)
    check_native(root)
    source(root)
    result = gates(root)
    consumers(root)
    ordinary(root)
    baseline = load(root / 'validation/semaphore-baseline-macos.json')
    need(baseline['exit-code'] == 101 and any('0 passed; 9 failed;' in result for result in baseline['results']), 'failing baseline')
    for name, expected in (('native_semaphore', 9), ('native_mutant', 11), ('native_single_wait', 16)):
        prototype = load(root / 'validation/semaphore-prototype-macos.json')
        row = next(row for row in prototype['steps'] if row['selection'] == name)
        need(row['exit-code'] == 0 and any(f'{expected} passed; 0 failed;' in result for result in row['results']), 'prototype cohort')
        log(root / 'validation', row)
    return {'bound-files': count, 'platforms': result, 'guest-exit-success': False, 'next-frontier': 'NtOpenKey snapshot coverage'}


if __name__ == '__main__':
    print(json.dumps(check(Path(__file__).resolve().parent), indent=2))
