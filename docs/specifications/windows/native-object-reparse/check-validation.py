"""Verify the selected final source, native/Unix gates, and retained carriers."""
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
    raw = path.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe', b'\xfe\xff']
                      else raw.decode('utf-8-sig'))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source(row):
    raw = (P / row['path']).read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
         'final source ' + row['path'])


def counts(name):
    return [tuple(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',
        (V / name).read_text(encoding='utf-8-sig'))]


need(__debug__, 'optimized validation rejected')
need((D / '.gitattributes').read_text() == '# Preserve raw native proof bytes on every host.\n* -text\n',
     'proof line-ending attributes')
for row in load(D / 'line-ending-verification.json')['files']:
    raw = (D / row['path']).read_bytes()
    blob = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
    need(row['equal'] and row['raw-git-blob'] == row['autocrlf-true-git-blob'] == blob,
         'line-ending preservation ' + row['path'])

seen = set()
for row in load(V / 'inputs.json')['files']:
    need(row['path'] not in seen, 'duplicate evidence path')
    seen.add(row['path'])
    raw = (D / row['path']).read_bytes()
    need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
         'evidence identity ' + row['path'])

manifest=load(V/'links-source-epoch1.json')
need(manifest['base']=='ce29b4a4bca74a668d7f029e8a69da6a2e0340e7' and manifest['source-epoch']==1,'published source parent')
need(len(manifest['files'])==len({r['path']for r in manifest['files']})==5743,'complete selected source inputs')
for row in manifest['files']:source(row)
comparison=load(V/'links-upstream-source-comparison.json')
need(comparison['matched-affected-files']==8 and comparison['frozen-carrier-abi-minor']==184,'retained/current source separation')
need(sum(r['equal']for r in comparison['files'])==8 and len(comparison['files'])==9,'current affected C++ identities')
for row in comparison['files']:
    need(row['equal']==(row['compared']==row['upstream']),'source comparison classification')
    need(row['compared'] in load(V/'current-cpp-consumers-links-root.json')['files'],'compared frozen source member')
    if not row['equal']:
        need(row['upstream']['path']=='src/crypto/static_text.cpp' and row['upstream']['sha256']=='1835211daeff4b7089081214c070f78f73606ec0f9a1f99cf448280402580372','separately reviewed static-text migration')

ignored = {'smir::lower::validation::tests::test_lift_lower_full_microkernel',
           'smir::lower::validation::tests::test_roundtrip_exact_bytes_microkernel'}
baseline_failures = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}
for host, total, failures in [('macos', 7934, 0), ('linux', 7928, 0), ('native', 7373, 5)]:
    name = 'links-' + host + '-epoch1-full.log'
    text = (V / name).read_text(encoding='utf-8-sig')
    need(counts(name) == [(total, failures, 2, 0)], 'full test counts ' + host)
    need(set(re.findall(r'^test (\S+) \.\.\. ignored$', text, re.M)) == ignored,
         'ignored test identities ' + host)
    if failures:
        failed = set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))
        need(failed == baseline_failures, 'native baseline failure identities')
    gates = load(V / ('links-' + host + '-epoch1-gates.json'))
    need([r['gate'] for r in gates] == (
        ['integration', 'capi', 'full', 'all-targets'] if host == 'native'
        else ['full', 'capi', 'integration', 'all-targets']), 'gate selection ' + host)
    for row in gates:
        command = ' '.join(row['argv']) if 'argv' in row else row['command']
        need(all(flag in command for flag in ['--locked', '--offline', '--no-default-features']),
             'core configuration ' + host + '/' + row['gate'])
        selection = {'focus': ['--lib', 'native_object_reparse'], 'full': ['--lib'], 'capi': ['-p rax-capi'],
                     'integration': ['--test user_windows', '--test user_windows_memory'],
                     'all-targets': ['check', '--all-targets']}[row['gate']]
        need(all(flag in command for flag in selection), 'core target selection')
        expected = 101 if host == 'native' and row['gate'] == 'full' else 0
        need(row['exit-code'] == expected, 'gate exit ' + host + '/' + row['gate'])
        if 'sha256' in row:
            need(digest(V / row['log']) == row['sha256'], 'gate log identity')
    need((168, 0, 0, 0) in counts('links-' + host + '-epoch1-capi.log'), 'C API count ' + host)
    expected = [(0, 0, 0, 0), (4, 0, 0, 0)] if host == 'native' else [
        (544, 0, 0, 0), (0, 0, 0, 0)]
    need(counts('links-' + host + '-epoch1-integration.log') == expected,
         'integration selection ' + host)

for host,log,passed,filtered in [
    ('macos','links-macos-focus-epoch1.log',5,7931),
    ('linux','links-linux-focus-epoch1.log',5,7925),
    ('native','links-native-focus-epoch1.log',6,7374),
]:
    need(counts(log)==[(passed,0,0,filtered)],'focused link tests '+host)
for name in ['links-native-full-epoch1.json','links-final-owned-launch.json']:
    row=load(V/name)
    need(row['state']=='finished' and row['exit-code']==0,'completed native driver')
need(load(V/'links-native-full-epoch1.json')['post-source-inputs']==5743,'native post-run source check')

for host, expected in [('macos', 7), ('linux', 4)]:
    gates = load(V / (host + '-links-owning-gates.json'))
    need(len(gates) == expected, 'owning gate count ' + host)
    for row in gates:
        need(row['exit-code'] == 0 and digest(V / row['log']) == row['sha256'],
             'owning gate and log ' + host)

for host in ['macos', 'linux', 'native']:
    archive = load(V / (host + '-links-final-owning-archive-identities.json'))
    need(archive['source-epoch'] == 'native-object-reparse-epoch1', 'owning source epoch')
    metadata = archive['compiler-messages']
    name = metadata['path'].replace('\\', '/').split('/')[-1]
    raw = (V / name).read_bytes()
    need(len(raw) == metadata['bytes'] and hashlib.sha256(raw).hexdigest() == metadata['sha256'],
         'owning Cargo message stream ' + host)
    messages = []
    for line in raw.decode('utf-8-sig').splitlines():
        try:
            messages.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    need(archive['core-artifact'] in messages and archive['carrier-artifact'] in messages,
         'actual Cargo-selected artifact records ' + host)
    need(archive['core-features'] == [] and archive['core-artifact']['features'] == [],
         'empty-feature core ' + host)
    need(len(archive['archives']) == 2 and len(archive['sources']) == 434,
         'owning archives and selected sources ' + host)
    for row in archive['sources']:
        source(row)
    for row in archive['archives']:
        need(row['structural-member-walk'] == 'pass' and row['members'] > 0
             and row['bytes'] > 0 and len(row['sha256']) == 64, 'complete archive walk')
    filenames = archive['core-artifact']['filenames'] + archive['carrier-artifact']['filenames']
    need(all(row['path'] in filenames for row in archive['archives']), 'actual Cargo artifacts')

ctest = (V / 'macos-links-owning-ctest.log').read_text()
passed = re.findall(r'^\d+/7 Test #\d+: (\S+)\s+\.+\s+Passed', ctest, re.M)
need(len(passed) == 7 and set(passed) == {
    'assist_rust_archive_link_tests', 'assist_rax_process_tests', 'assist_process_tool_tests',
    'assist_process_tool_unavailable_tests', 'assist_e14_rax_abi_drift_tests',
    'assist_mcp_tool_manifest_tests', 'assist_mcp_manifest_runtime_test',
}, 'macOS seven CTest outcomes')
for host, name, process_checks in [('native', 'assist-native-links-final-cpp.log', 182),
                                    ('linux', 'linux-links-owning-cpp.log', 184)]:
    text = (V / name).read_text(encoding='utf-8-sig')
    for token in [str(process_checks) + ' checks passed', '162 checks passed', '2/2 passed',
                  'PASS', 'canonical discovery and explicit RAX-disabled refusal passed']:
        need(token in text, 'C++ consumer ' + host + '/' + token)

expected_programs=['325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b','48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6']
terminals=[f'terminal turn={turn} Complete(Internal("native NTDLL service NtCreateMutant (0xba) at 0x7ff82fad1be0 is not implemented"))'for turn in [2150070,2152419]]
owner = load(V / 'links-owner-observer-launch.json')
need(owner['state'] == 'finished' and owner['exit_code'] == 0, 'owner diagnostic completed')
for label in ['source', 'executable', 'archive']:
    before = owner[label + '-before'] if label != 'archive' else owner['archive']
    after = owner[label + '-after']
    need(before['sha256'] == after['sha256'] and before['bytes'] == after['bytes'],
         'owner diagnostic before/after identity ' + label)
need(owner['source-before']['sha256'] ==
     '1a5d047c324cdcc365d697273bacca64536d0c4366ee58dc0e66764119efa907',
     'unchanged fast diagnostic source')
need(digest(V / 'owner-diagnostic/links-owner-observer.rs') == owner['source-before']['sha256'],
     'retained diagnostic source bytes')
archives = load(V / 'native-links-final-owning-archive-identities.json')['archives']
need(any(row['sha256'] == owner['archive']['sha256'] and row['path'] == owner['archive']['path']
         for row in archives), 'diagnostic linked to selected retained-owner archive')
need(len(owner['outcomes']) == 2, 'two owner diagnostic targets')
for index, row in enumerate(owner['outcomes']):
    need(row['exit-code'] == 0 and row['program-before'] == row['program-after'],
         'owner diagnostic target unchanged')
    need(row['program-before']['sha256'] == expected_programs[index],
         'owner diagnostic same target')
    text = (V / row['log']).read_text(encoding='utf-8-sig')
    need(digest(V / row['log']) == row['log-identity']['sha256'], 'owner diagnostic log identity')
    need('prefix-turns=2100000 result=BudgetExhausted' in text and
         terminals[index] in text, 'owner diagnostic same measured terminal')



print('Final link source, three-host gates, retained consumers and unchanged startup frontier verified')
