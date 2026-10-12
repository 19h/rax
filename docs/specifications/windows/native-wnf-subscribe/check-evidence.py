"""Check retained native WNF subscriptions, fault ordering and producer identities."""
from pathlib import Path
import hashlib, itertools, json, re, sys

def require(ok, message):
    if not ok:
        raise ValueError(message)
require(not sys.flags.optimize, 'optimized validation is rejected')
base = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
source_root = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).resolve().parents[4]
def read(name): return (base / name).read_text(encoding='utf-8-sig')
def data(name): return json.loads(read(name))
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def rows(name, prefix='case='):
    return [dict(f.split('=', 1) for f in (line.split()[1:] if prefix.endswith(' ') else line.split())) for line in read(name).splitlines() if line.startswith(prefix)]
def num(row, key, radix=16): return int(row[key], radix)
def results(name):
    return [tuple(map(int, match)) for match in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', read(name))]
manifest = data('evidence-manifest.json')
require(set(manifest) == {p.name for p in base.iterdir() if p.is_file() and p.name != 'evidence-manifest.json'}, 'exact evidence coverage')
for name, row in manifest.items():
    require((base / name).stat().st_size == row['bytes'] and sha(base / name) == row['sha256'], 'evidence identity ' + name)
profiles = ['arm64', 'x64', 'x86', 'x86-laa']
families = ['', '-boundaries', '-alias', '-domain', '-guards']
expected = {'native-wnf-subscribe' + family + '-' + profile for family in families for profile in profiles}
expected |= {'native-wnf-subscribe-' + family + '-arm64' for family in ['fresh', 'update', 'delivery', 'pending', 'process-names', 'process-fresh', 'subject']}
identity = data('native-wnf-subscribe-producer-identities.json')
require(identity['windows-version'] == '10.0.29683' and identity['host-machine'].upper() == 'ARM64', 'actual installed host')
require(len(identity['producers']) == 27 and {r['profile'] for r in identity['producers']} == expected, 'exact native producer selection')
for row in identity['producers']:
    source = row['source_identity']; log = row['log_identity']; exe = row['executable_identity']
    require(source['sha256'] == row['source-sha256'] == sha(base / row['source']), 'actual compiled source')
    require(source['bytes'] == (base / row['source']).stat().st_size, 'compiled source size')
    require(log['sha256'] == sha(base / row['log']) and log['bytes'] == (base / row['log']).stat().st_size, 'actual producer stdout')
    profile = next(p for p in sorted(profiles, key=len, reverse=True) if row['profile'].endswith('-' + p))
    require(exe['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile], 'actual PE architecture')
    require(exe['bytes'] > 0 and re.fullmatch('[a-f0-9]{64}', exe['sha256']), 'actual PE identity')
    if profile == 'x86-laa': require(exe['characteristics'] & 0x20, 'actual PE LAA flag')
require([r['sha256'] for r in identity['native-libraries']] == [
    '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'], 'pinned installed NTDLL')
KNOWN = 0x41911A28A3BC10B5
XOR = 0x41C64E6DA3BC0074
INVALID, MISSING, DENIED, AV, GUARD = 0xC000000D, 0xC0000034, 0xC0000022, 0xC0000005, 0x80000001
POISON = 0xA5A5A5A5A5A5A5A5
def valid(name):
    bits = name ^ XOR
    scope = (bits >> 6) & 15
    return bits & 15 == 1 and scope <= 5 and (not bits & 0x400 or scope in [0, 2, 4])
def status(name, mask): return INVALID if not valid(name) or mask > 31 else 0 if name == KNOWN else MISSING
def levels(mask): return 8 | (2 if mask & 1 else 4 if mask & 14 else 0)
total = 0
for profile in profiles:
    stem = 'native-wnf-subscribe-' + profile + '.log'
    observed = rows(stem)
    require(len(observed) == 90 and 'complete cases=90' in read(stem), 'complete basic matrix')
    cases = list(itertools.product([KNOWN, 0, (1 << 64) - 1, KNOWN ^ 1, KNOWN ^ 16, KNOWN ^ 64], [0, 1, 0xFFFFFFFF], [0, 1, 16, 17, 0xFFFFFFFF]))
    require([(num(r, 'name'), num(r, 'stamp'), num(r, 'mask')) for r in observed] == cases, 'exact basic partitions')
    for r in observed:
        expected_status = status(num(r, 'name'), num(r, 'mask'))
        require(num(r, 'status') == num(r, 'repeat') == expected_status, 'basic admission status')
        require(num(r, 'id') == num(r, 'repeat-id') and (num(r, 'id') > 0) == (expected_status == 0), 'opaque identity stability')
        require(r['handles-before'] == r['handles-after'], 'native handle lifetime')
    total += 90
    stem = 'native-wnf-subscribe-domain-' + profile + '.log'; observed = rows(stem)
    require(len(observed) == 2048 and 'complete cases=2048' in read(stem), 'complete syntax domain')
    require([(num(r, 'version', 10), num(r, 'lifetime', 10), num(r, 'scope', 10), num(r, 'permanent', 10)) for r in observed] == list(itertools.product(range(16), range(4), range(16), range(2))), 'exact syntax partitions')
    for r in observed: require(num(r, 'status') == status(num(r, 'name'), 17), 'syntax admission status')
    total += 2048
    stem = 'native-wnf-subscribe-boundaries-' + profile + '.log'
    names = rows(stem, 'name-case='); pointers = rows(stem, 'pointer-case='); updates = rows(stem, 'sequence-case=')
    require(len(names) == 4288 and len(pointers) == 624 and len(updates) == 64 and 'complete cases=4976' in read(stem), 'complete boundary matrix')
    require([(num(r, 'bit', 10), num(r, 'mask')) for r in names] == list(itertools.product([64, 65, 66] + list(range(64)), range(64))), 'exact name/mask partitions')
    for r in names: require(num(r, 'status') == status(num(r, 'name'), num(r, 'mask')), 'boundary name admission')
    require([(num(r, 'input', 10), num(r, 'output', 10), num(r, 'mask')) for r in pointers] == list(itertools.product(range(13), range(12), [0, 17, 32, 0xFFFFFFFF])), 'exact pointer partitions')
    for r in pointers:
        ik, ok, mask = num(r, 'input', 10), num(r, 'output', 10), num(r, 'mask')
        expected_status = AV if ik in [1, 2, 3, 4] or ok in [2, 3, 4] else INVALID if mask > 31 else 0
        require(num(r, 'status') == expected_status, 'pointer status precedence')
        if ok == 4: require(num(r, 'after') == POISON, 'crossing output unchanged')
        if ok == 0 or ok >= 5: require((num(r, 'after') != 0) == (expected_status == 0), 'writable output publication')
    masks = [0, 1, 2, 4, 8, 16, 17, 31]
    require([(num(r, 'first'), num(r, 'second')) for r in updates] == list(itertools.product(masks, masks)), 'all request update pairs')
    for r in updates:
        require(r['status'] == r['update'] == r['end'] == '00000000' and r['twice'] == 'C0000034', 'update/unsubscribe lifecycle')
        require(num(r, 'id') == num(r, 'update-id') > 0 and num(r, 'present') == (num(r, 'second') & 1), 'stable current request identity')
    total += 4976
    stem = 'native-wnf-subscribe-guards-' + profile + '.log'; observed = rows(stem)
    require(len(observed) == 210 and 'complete cases=210' in read(stem), 'complete guard matrix')
    require([(num(r, 'input', 10), num(r, 'output', 10), num(r, 'mask')) for r in observed] == list(itertools.product(range(5), range(14), [0, 17, 32])), 'exact guard partitions')
    for r in observed:
        ik, ok, mask = num(r, 'input', 10), num(r, 'output', 10), num(r, 'mask')
        wow = profile.startswith('x86'); ip = 0x104 if ik == 1 else 1 if ik == 2 else 4
        op = 0x104 if ok == 1 else 1 if ok == 2 else 2 if ok == 4 else 4
        upper = [2, 0x104, 1][(ok - 5) % 3] if ok >= 5 else 4
        crossing_prefix = ok >= 8
        early = ok in [1, 2] or (not wow and (ok == 4 or ok >= 5)) or (wow and crossing_prefix and upper in [0x104, 1])
        created = False
        if early:
            fault = op if ok < 5 else upper
            expected_status = GUARD if fault == 0x104 else AV
            if fault == 0x104:
                if ok < 5: op = 4
                else: upper = 4
        else:
            expected_status = GUARD if ik == 1 else AV if ik in [2, 3] else INVALID if ik == 4 or mask > 31 else 0
            if ik == 1: ip = 4
            created = expected_status == 0
            if wow and (ok == 4 or ok >= 5):
                expected_status = GUARD if upper == 0x104 else AV
                if upper == 0x104: upper = 4
        require(num(r, 'status') == expected_status, 'guard/copy status order')
        require((num(r, 'input-protect'), num(r, 'output-protect'), num(r, 'upper-protect')) == (ip, op, upper), 'one-shot guard consumption')
        require(num(r, 'end') == (0 if created else MISSING), 'publication surviving failed copy')
        output_fault = early or (wow and (ok == 4 or ok >= 5))
        if ok == 3 or output_fault:
            require(num(r, 'after') == POISON, 'failed/null output byte extent')
        else:
            require((num(r, 'after') > 0) == created, 'complete zero/identifier output')
    total += 210
require(total == 29296, 'exact mature cases excluding aliases')
for profile in profiles:
    stem = 'native-wnf-subscribe-alias-' + profile + '.log'; observed = rows(stem, 'alias-case=')
    require(len(observed) == 57 and 'complete alias-cases=57 data-pairs=8' in read(stem), 'complete alias matrix')
    require([(num(r, 'delta', 10), num(r, 'mask')) for r in observed] == list(itertools.product(range(-9, 10), [0, 17, 32])), 'exact overlap partitions')
    for r in observed:
        delta = num(r, 'delta', 10); memory = bytearray([0xA5] * 64)
        memory[24:32] = KNOWN.to_bytes(8, 'little')
        if not profile.startswith('x86'): memory[24 + delta:32 + delta] = bytes(8)
        captured = KNOWN if profile.startswith('x86') else int.from_bytes(memory[24:32], 'little')
        expected_status = status(captured, num(r, 'mask'))
        require(num(r, 'status') == expected_status and (num(r, 'id') > 0) == (expected_status == 0), 'alias capture/clear ordering')
        memory[24 + delta:32 + delta] = num(r, 'id').to_bytes(8, 'little')
        require(num(r, 'name-after') == int.from_bytes(memory[24:32], 'little'), 'exact relative identifier footprint')
        require(num(r, 'end') == (0 if expected_status == 0 else MISSING), 'alias publication lifetime')
    total += 57
require(total == 29524, 'exact mature subscription cases')
process_names = {
    0x02821B2CA3BC08B5: 1, 0x0D920D2EA3BC08B5: 0, KNOWN: 3,
    0x41960A28A3BC50B5: 3, 0x41C61629A3BC30B5: 1,
    0x41C61629A3BC60B5: 1, 0x41C61629A3BCA8B5: 0,
    0x41C61629A3BD10B5: 1, 0x41C61D3DA3BC10B5: 3}
def admitted(name, mask):
    grant = process_names[name]
    required = 0 if mask == 0 else 3 if mask & 14 else 1
    return 0 if grant & required == required else DENIED
catalog = data('native-wnf-subscribe-registry.json')
require(len(catalog) == 1613 and len({r['name'] for r in catalog}) == 1613, 'captured provisioning catalog')
require({int(r['name'], 16) for r in catalog if ((int(r['name'], 16) ^ XOR) >> 6) & 15 == 3} == set(process_names), 'exact selected process names')
subject = read('native-wnf-subscribe-subject-arm64.log')
require('bytes=010100000000000512000000' in subject and 'app-container=0' in subject and 'handles-before=59 handles-after=59' in subject, 'captured primary subject/lifetime')
for family, count, calls in [('fresh', 64, 4), ('update', 128, 4), ('delivery', 128, 6), ('pending', 128, 6), ('process-fresh', 288, 6)]:
    stem = 'native-wnf-subscribe-' + family + '-arm64.log'; observed = rows(stem)
    require(len(observed) == count + 1 and [num(r, 'case', 10) for r in observed] == list(range(count + 1)), 'exact fresh ordinals ' + family)
    require(observed[0]['warmup'] == '1' and all(r['warmup'] == '0' for r in observed[1:]), 'one excluded debugger warmup ' + family)
    traps = rows(stem, 'trap ')
    require(len(traps) == (count + 1) * (calls + 1), 'every entry/return breakpoint ' + family)
    for ordinal in range(count + 1):
        require([r['phase'] for r in traps if num(r, 'case', 10) == ordinal] == ['entry'] + [str(i) for i in range(calls)], 'exact breakpoint phases')
    for r in observed[1:]:
        require(r['calls'] == str(calls) and r['exit'] == '77' and r['width'] == '8' and r['registered'] == '00000000', 'native child operation completion')
        require(r['handles-before'] == r['handles-after'], 'native child/alias handle restoration')
        name = num(r, 'name') if family == 'process-fresh' else KNOWN
        expected_status = admitted(name, num(r, 'mask'))
        require(num(r, 'status') == num(r, 'repeat') == expected_status, 'fresh subject admission')
        require(num(r, 'id') == num(r, 'repeat-id') and (num(r, 'id') > 0) == (expected_status == 0), 'fresh opaque identity')
        require(num(r, 'end') == (0 if expected_status == 0 else MISSING), 'fresh unsubscribe status')
    if family == 'fresh':
        require([(num(r, 'after', 10), num(r, 'mask')) for r in observed[1:]] == list(itertools.product(range(2), range(32))), 'all registration orders/masks')
        for r in observed[1:]:
            signal = int(r['after'] == '1' and bool(num(r, 'mask') & levels(num(r, 'mask'))))
            require(all(num(r, 'state' + str(i), 10) == signal for i in [2, 3, 4]), 'no retroactive signal; sticky repeat/unsubscribe')
    if family in ['delivery', 'pending', 'process-fresh']:
        descriptors = rows(stem, 'descriptor ')
        require(len(descriptors) == (count + 1) * 2, 'all native descriptor captures')
        by_case = {(r['case'], r['index']): r for r in descriptors}
        require(len(by_case) == len(descriptors), 'unique descriptor selection')
        for r in observed[1:]:
            name = num(r, 'name') if family == 'process-fresh' else KNOWN
            first, second = num(r, 'mask'), num(r, 'next-mask')
            expected_mask = first & levels(first)
            index = '0'
            if family == 'pending':
                expected_mask = (expected_mask & second) | (second & levels(second) & (~first | ~levels(first)))
                require(by_case[(r['case'], '0')]['status'] == 'C0000023', 'small first buffer cannot dequeue')
                index = '1'
            if num(r, 'status') != 0: expected_mask = 0
            d = by_case[(r['case'], index)]
            require(num(d, 'status') == (0 if expected_mask else 0x8000001A), 'metadata/empty queue status')
            if expected_mask:
                raw = bytes.fromhex(d['bytes'])
                require(len(raw) == 48 and int.from_bytes(raw[8:16], 'little') == name and raw[16:24] == bytes(8) and raw[28:44] == bytes(16), 'zero initial payload/stamp/type')
                require(int.from_bytes(raw[24:28], 'little') == expected_mask and int.from_bytes(raw[44:48], 'little') == 48, 'exact metadata/offset')
    total += count
require(total == 30260, 'mature plus complete fresh subscription cases')
stem = 'native-wnf-subscribe-process-names-arm64.log'
observed = rows(stem, 'data-after ')
require(len(observed) == 72 and 'complete names=9 data-pairs=72' in read(stem), 'all mature process-name calls')
require([(num(r, 'name'), num(r, 'mask')) for r in observed] == list(itertools.product(process_names, [0, 1, 2, 4, 8, 16, 17, 31])), 'exact process-name/access partitions')
for r in observed:
    require(num(r, 'subscribed') == admitted(num(r, 'name'), num(r, 'mask')), 'captured primary-subject permission')
total += 72
require(total == 30332, 'exact native subscription cases excluding five warmups')

prefix = 'native-wnf-subscribe-'
EXPECTED_FULL = {'macos': (7674, 0, 2), 'linux': (7666, 2, 2), 'windows': (7078, 5, 2)}
summary = data('validation-summary.json')
require(summary['native-cases'] == 30332 and summary['native-warmups-excluded'] == 5, 'stdout-bound native summary')
required = {'targeted', 'previous-wnf-event', 'binding', 'soft-maximum', 'hard-maximum', 'wait-packet-associate', 'wait-packet-create', 'threading', 'timer2', 'worker-factory', 'io-completion', 'events', 'full', 'capi', 'all-targets', 'integration'}
for host in ['macos', 'linux', 'windows']:
    stem = host + '-' if host != 'windows' else prefix
    gates = data(host + '-gates.json' if host != 'windows' else prefix + 'gates.json')
    require(len(gates) == 16 and {r['gate'] for r in gates} == required, 'all sixteen owning host gates ' + host)
    require(results(stem + 'full.log') == [EXPECTED_FULL[host]], 'actual full-suite counts ' + host)
    require(summary['tests']['full'][host] == dict(zip(['pass', 'fail', 'ignored'], EXPECTED_FULL[host])), 'full-suite summary identity')
    for r in gates: require(r['exit-code'] == (101 if r['gate'] == 'full' and EXPECTED_FULL[host][1] else 0), 'actual gate exit ' + host + '/' + r['gate'])
    require(results(stem + 'targeted.log') == [(15 if host == 'windows' else 14, 0, 0)], 'nonempty focused subscription selection')
    require(summary['tests']['targeted'][host] == (15 if host == 'windows' else 14), 'focused summary identity')
    require(results(stem + 'previous-wnf-event.log') == [(8 if host == 'windows' else 7, 0, 0)], 'previous Event ownership regression')
    require(results(stem + 'capi.log')[0] == (168, 0, 0), 'all C API tests')
    require(results(stem + 'integration.log')[0] == (4 if host == 'windows' else 544, 0, 0), 'registered integration count')
    require('--no-default-features' in next(r['command'] for r in gates if r['gate'] == 'all-targets'), 'selected empty-feature build')
expected_failures = {
    'linux': ['user::linux::tests::uring::timeout::a_multishot_timeout_reports_each_expiry', 'user::linux::tests::uring::timeout::a_timeout_is_removed_or_updated'],
    'windows': ['smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry', 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry', 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry', 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards', 'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime']}
for host, names in expected_failures.items():
    stem = host + '-' if host != 'windows' else prefix
    failure_text = read(stem + 'full.log').rsplit('\nfailures:\n', 1)[1].split('\ntest result:', 1)[0]
    require(failure_text.split() == names, 'exact unchanged full-suite failures ' + host)
isolated = data('linux-isolated-gates.json')
require(len(isolated) == 2 and {r['test'] for r in isolated} == {'a_timeout_is_removed_or_updated', 'a_multishot_timeout_reports_each_expiry'}, 'all timeout isolates')
for r in isolated: require(r['exit-code'] == 0 and '--exact' in r['command'] and results('linux-isolated-' + r['test'] + '.log') == [(1, 0, 0)], 'nonempty passing timeout isolate')
require('--test-threads=1' in next(r['command'] for r in data('linux-gates.json') if r['gate'] == 'integration'), 'serial Linux integration')
for host, count in [('macos', 5), ('linux', 2)]:
    gates = data(host + '-owning-gates.json')
    require(len(gates) == count and all(r['exit-code'] == 0 for r in gates), 'owning product gates ' + host)
require('100% tests passed out of 7' in read('macos-owning-ctest.log'), 'seven owning CTest selections')
for name in ['macos-owning-protected-assist-cli.log', 'macos-owning-protected-assist-mcp.log']:
    require('0 failures' in read(name), 'owning protected text scan')
for host, name in [('linux', 'linux-owning-cpp.log'), ('windows', 'assist-native-wnf-subscribe-owned-cpp.log')]:
    text = read(name)
    for target in ['process_tool', 'process_tool_unavailable', 'rax_process', 'rax_abi_drift', 'rust_archive_link']:
        require(('run /validation/' + target + ' /validation/' + target) in text if host == 'linux' else target + '.exe' in text, 'actual C++ consumer ' + host + '/' + target)
archive = data(prefix + 'owning-archive-hashes.json'); frozen = data('owning-source-hashes.json')
require(archive['core-features'] == [] and len(archive['sources']) == 56 and len(archive['helpers']) == 2, 'actual owning source/core-feature selection')
require(archive['sources'] == frozen['sources'] and archive['helpers'] == frozen['helpers'], 'frozen source/helper identity')
for r in archive['sources']:
    path = source_root / r['path']
    require(path.is_file() and path.stat().st_size == r['bytes'] and sha(path) == r['sha256'], 'published compiled source ' + r['path'])
require([r['sha256'] for r in archive['archives']] == ['0e75e2348d5ec6358118cc038fe4ab55afea6c6cb52e5f5717abdffdd91231a3', '7d7e5883c6c61db0596c81fa624ea3f333818e9b9921bd611ef7948c8eccbb71'], 'actual native archive identity')
require(all(r['structural-member-walk'] == 'pass' and r['members'] > 0 for r in archive['archives']), 'complete owning archives')
applied = data(prefix + 'current-source-check.json')
require(applied['errors'] == [] and applied['source-files'] == 17250 and applied['patches'] == 152, 'actual applied native source snapshot')
require(read('root-owning-source-delta.txt').splitlines() == ['VENDORED_VERSIONS.md', 'src/emulation/native-process-emulation.md'], 'Unix owning root compiled-code parity')
require(sha(base / (prefix + 'after-trace.rs')) == 'd41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e', 'unchanged production observer')
require('terminal turn=46245 Complete(Internal(' in read('before-production-trace.log') and 'NtSubscribeWnfStateChange' in read('before-production-trace.log'), 'observed previous frontier')
frontier = {'before-turn': 46245, 'after-turn': 50254, 'delta-scheduler-calls': 4009, 'service': 'NtTraceControl', 'svc': '0x1d6', 'pc': '0x180002da0', 'thread': 8, 'arguments': ['0x1b', '0xabe800', '0x4', '0x0', '0x0', '0xabe804'], 'guest-exit': 'not reached'}
require(summary['frontier'] == frontier, 'bounded frontier summary')
trace = read(prefix + 'after-production-trace-output.log')
require('terminal turn=50254 Complete(Internal(' in trace and 'NtTraceControl (0x1d6) at 0x180002da0 is not implemented' in trace, 'actual next unsupported service')
require('turn=50254 thread=8 PC=0x180002da0' in trace and 'registers=[1b, abe800, 4, 0, 0, abe804,' in trace, 'actual service identity/arguments')
ordinary = read(prefix + 'after-ordinary-processes.log')
require(ordinary.count('program ') == 4 and ordinary.count('run reason=5 exit=0 diagnostic=') == 4 and ordinary.count('NtTraceControl') == 4, 'all ordinary guest failures retained')
print('pass: 27 native producers; 30332 subscription cases; 48 host gates; owning/source/archive identities; next unsupported NtTraceControl')
