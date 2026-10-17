"""Check exact core, native, carrier and incomplete-application evidence."""
from pathlib import Path
import argparse
import hashlib
import json
import re

D = Path(__file__).resolve().parent
V = D / 'validation'
parser = argparse.ArgumentParser()
parser.add_argument('--source-root', type=Path, default=D.parents[3])
P = parser.parse_args().source_root


def need(ok, message):
    if not ok:
        raise ValueError(message)


def load(path):
    data = path.read_bytes()
    return json.loads(data.decode('utf-16') if data[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else data.decode('utf-8-sig'))


def counts(name):
    data = (V / name).read_text(encoding='utf-8-sig')
    return [tuple(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;', data)]


def source(row):
    data = (P / row['path']).read_bytes()
    need(len(data) == row['bytes'] and hashlib.sha256(data).hexdigest() == row['sha256'],
         'final source ' + row['path'])


need(__debug__, 'optimized validation is rejected')
seen = set()
for row in load(V / 'inputs.json')['files']:
    need(row['path'] not in seen, 'duplicate evidence path')
    seen.add(row['path'])
    data = (D / row['path']).read_bytes()
    need(len(data) == row['bytes'] and hashlib.sha256(data).hexdigest() == row['sha256'],
         'evidence identity ' + row['path'])

ignored = {'smir::lower::validation::tests::test_lift_lower_full_microkernel',
           'smir::lower::validation::tests::test_roundtrip_exact_bytes_microkernel'}
for host in ['macos', 'linux', 'native']:
    data = (V / (host + '-system-time-final-full.log')).read_text(encoding='utf-8-sig')
    need(set(re.findall(r'^test (\S+) \.\.\. ignored$', data, re.M)) == ignored,
         'ignored test identities ' + host)

bridge = load(V / 'core-source-bridge.json')
changed = 'src/user/windows/process/sched/tests/services_tests/image_extension_owner_leaf_tests.rs'
thread_changed = 'src/user/windows/process/thread.rs'
need(bridge['changed-paths'] == [changed, thread_changed], 'only two test files differ')
thread_old = (V / 'thread-before-epoch7.rs').read_bytes()
thread_new = (P / thread_changed).read_bytes()
marker = b'#[cfg(test)]\nmod tests {'
need(thread_old.split(marker)[0] == thread_new.split(marker)[0], 'unchanged thread production prefix')
need(hashlib.sha256(thread_old).hexdigest() == bridge['thread-test-change']['before-sha256'], 'original thread test identity')
snapshot = load(V / 'time-full-snapshot-source-inputs.json')['files']
need(len(snapshot) == 18805 and len({r['path'] for r in snapshot}) == 18805,
     'complete original snapshot input set')
original_rust = {r['path']: r for r in snapshot if r['kind'] == 'file' and r['path'].endswith('.rs')}
need(len(original_rust) == len(bridge['final-inputs']) == 5517, 'complete Rust source bridge')
for row in bridge['final-inputs']:
    original = original_rust[row['path']]
    if row['path'] == changed:
        need(original['sha256'] == bridge['before-sha256'], 'original image leaf snapshot')
    elif row['path'] == thread_changed:
        need(original['sha256'] == bridge['thread-test-change']['before-sha256'],
             'original TLS fixture snapshot')
    else:
        need(original['sha256'] == row['sha256'] and original['bytes'] == row['bytes'],
             'unchanged compiled source ' + row['path'])
need('#[cfg(windows)]' in (P / 'src/user/windows/process/sched/tests/services_tests.rs').read_text(),
     'Windows-only test parent')
for row in bridge['final-inputs']:
    source(row)
old = (V / 'image-owner-leaf-before.rs').read_bytes()
need(hashlib.sha256(old).hexdigest() == bridge['before-sha256'], 'old leaf identity')
need(hashlib.sha256((P / changed).read_bytes()).hexdigest() == bridge['after-sha256'],
     'inherited final leaf identity')

for host, total in [('macos', 7880), ('linux', 7874)]:
    gates = load(V / (host + '-system-time-final-gates.json'))
    need([r['gate'] for r in gates] == ['focus', 'full', 'capi', 'integration', 'all-targets'],
         'Unix gate selection ' + host)
    for row in gates:
        need(row['exit-code'] == 0, 'Unix gate failure')
        need(hashlib.sha256((V / row['log']).read_bytes()).hexdigest() == row['sha256'],
             'Unix gate digest')
    need(counts(host + '-system-time-final-focus.log')[0][:3] == (2, 0, 0), 'Unix focus count')
    need(counts(host + '-system-time-final-full.log') == [(total, 0, 2, 0)], 'Unix full count')
    need((168, 0, 0, 0) in counts(host + '-system-time-final-capi.log'), 'Unix C API count')
    need(counts(host + '-system-time-final-integration.log') == [(544, 0, 0, 0), (0, 0, 0, 0)],
         'Unix integration selection')
    for row in load(V / (host + '-time-owning-gates.json')):
        need(row['exit-code'] == 0, 'Unix owning gate')
        need(hashlib.sha256((V / row['log']).read_bytes()).hexdigest() == row['sha256'],
             'Unix owning digest')

need(counts('native-system-time-final-focus-focus.log') == [(3, 0, 0, 7316)], 'native initial focus')
need(counts('native-system-time-epoch7-focus.log') == [(3, 0, 0, 7316)], 'native final clock focus')
need(counts('native-system-time-epoch7-image-focus.log') == [(20, 0, 0, 7299)],
     'native inherited image focus')
for name in ['system-time-isolated-v5-launch.json', 'system-time-epoch7-owning-launch.json']:
    record = load(V / name)
    need(record['state'] == 'finished' and record['exit-code'] == 0, 'native driver ' + name)
need(all(r['exit-code'] == 0 for r in load(V / 'native-system-time-epoch7-gates.json')),
     'native final compile gates')
rows = load(V / 'native-system-time-final-gates.json')
need([r['gate'] for r in rows] == ['integration', 'capi', 'full', 'all-targets'], 'native gate selection')
for row in rows:
    need(row['exit-code'] == (101 if row['gate'] == 'full' else 0), 'native gate exit')
need(counts('native-system-time-final-full.log') == [(7310, 7, 2, 0)], 'native full count')
data = (V / 'native-system-time-final-full.log').read_text(encoding='utf-8-sig')
failed = set(re.findall(r'^    (\S+)$', data[data.rfind('\nfailures:\n'):], re.M))
need(failed == {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
    'user::windows::process::thread::tests::inaccessible_tls_template_releases_stack_teb_and_heap_blocks',
    'user::windows::process::thread::tests::overflowing_tls_template_size_and_invalid_index_are_rejected',
}, 'native baseline failure identities')
need((168, 0, 0, 0) in counts('native-system-time-final-capi.log'), 'native C API count')
need(counts('native-system-time-final-integration.log') == [(0, 0, 0, 0), (4, 0, 0, 0)],
     'native integration selection')

for host in ['macos', 'linux', 'native']:
    archive = load(V / (host + '-time-final-owning-archive-identities.json'))
    need(archive['core-features'] == [] and len(archive['archives']) == 2, 'empty-feature carrier')
    need(len(archive['sources']) == 354, 'selected owning source count')
    for row in archive['sources']:
        if host != 'native' and row['path'] == thread_changed:
            need(row['sha256'] == bridge['thread-test-change']['before-sha256'] and row['bytes'] == len(thread_old), 'retained carrier original test identity')
        else:
            source(row)
    for row in archive['archives']:
        need(row['structural-member-walk'] == 'pass' and row['members'] > 0 and row['bytes'] > 0,
             'complete archive member walk')
for host in ['macos', 'linux', 'native']:
    gates = load(V / (host + '-system-time-epoch7-gates.json'))
    need([gate['gate'] for gate in gates] == (
        ['focus', 'thread-focus', 'heap-focus', 'image-focus', 'all-targets'] if host == 'native'
        else ['thread-focus', 'heap-focus', 'all-targets']), 'final epoch7 gate selection')
    for gate in gates:
        need(gate['exit-code'] == 0, 'epoch7 gate failure')
        if 'sha256' in gate:
            need(hashlib.sha256((V / gate['log']).read_bytes()).hexdigest() == gate['sha256'],
                 'epoch7 gate digest')
    need(counts(host + '-system-time-epoch7-thread-focus.log')[0][0:3] == (3, 0, 0), 'final TLS rollback tests')
    heap_counts = counts(host + '-system-time-epoch7-heap-focus.log')
    need(len(heap_counts) == 1 and heap_counts[0][0:3] == (17, 0, 0), 'heap retained-page contract tests')
cpp = (V / 'assist-native-system-time-final-cpp.log').read_text()
for token in ['182 checks passed', '162 checks passed', '2/2 passed', 'PASS',
              'canonical discovery and explicit RAX-disabled refusal passed']:
    need(token in cpp, 'native C++ consumer ' + token)
ctest = (V / 'macos-time-owning-ctest.log').read_text()
passed_ctests = re.findall(r'^\d+/7 Test #\d+: (\S+)\s+\.+\s+Passed', ctest, re.M)
need(len(passed_ctests) == 7 and set(passed_ctests) == {
    'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
    'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
    'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test',
}, 'macOS seven CTest identities and successful outcomes')

ordinary = load(V / 'ordinary-analysis.json')
executed = [r for r in ordinary if 'terminal-lines' in r]
need(len(executed) == 2, 'ordinary execution count')
for row, turn in zip(executed, [97977, 98041]):
    need(row['controller-exit-code'] == 0, 'ordinary controller status')
    need(len(row['terminal-lines']) == 1 and f'terminal turn={turn} ' in row['terminal-lines'][0]
         and 'Complete(Internal(' in row['terminal-lines'][0]
         and 'NtSetInformationProcess class 53' in row['terminal-lines'][0],
         'ordinary incomplete boundary')
    need(hashlib.sha256((V / row['log']).read_bytes()).hexdigest() == row['sha256'],
         'ordinary raw log identity')
print('System-time validation: exact sources, native/Unix gates, carriers and incomplete ordinary results verified')
