"""Exclusive native cookie core/startup and owning-carrier validation."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import traceback

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
RECORD = SHARE / 'token-query-epoch2-launch.json'
LOCK = BASE / 'token-query-epoch2-driver.lock'
KNOWN = {
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
    'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
    'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}


def require(ok, text):
    if not ok:
        raise ValueError(text)


def load(name):
    return json.loads((SHARE / name).read_text(encoding='utf-8-sig'))


def verify_core():
    gates = load('native-token-query-epoch2-gates.json')
    require(len(gates) == 7 and all(g['exit-code'] == (101 if g['gate'] == 'full' else 0) for g in gates),
            'seven completed native core selections; only the known full-library failures allowed')
    summaries = {}
    for gate in gates:
        path = SHARE / gate['log']
        text = path.read_text(encoding='utf-8-sig')
        summaries[gate['gate']] = [list(map(int, x)) for x in re.findall(
            r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;', text)]
        gate['sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
        if gate['gate'] == 'full':
            names = set(re.findall(r'^    (\S+)$', text[text.rfind('\nfailures:\n'):], re.M))
            require(names == KNOWN and summaries['full'] == [[7267, 5, 2, 0]],
                    'actual complete library count and exact retained lowerer failures')
    require(any(x[:3] == [3, 0, 0] for x in summaries['focused']), 'three token regressions including actual installed leaves')
    require(any(x[:3] == [1, 0, 0] for x in summaries['subject']), 'one captured subject regression')
    require(any(x[:3] == [3, 0, 0] for x in summaries['cookie']), 'three retained cookie regressions')
    require([168, 0, 0, 0] in summaries['capi'], '168 native C API tests')
    require([4, 0, 0, 0] in summaries['integration'], 'four native memory integration tests')
    result = {'gates': gates, 'summaries': summaries,
              'scope': 'published cookie parent plus ten token source inputs and canonical native fixture; no ordinary success inference'}
    (SHARE / 'native-token-query-epoch2-verified-gates.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    require(__debug__, 'optimized verification rejected')
    require(not RECORD.exists(), 'native cookie driver already requested; inspect its actual state before retry')
    for name in ['cookie-owner-final-owning-restored.json']:
        require(load(name).get('exit-code') == 0, 'completed parent native job')
    fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    os.write(fd, str(os.getpid()).encode())
    os.close(fd)
    row = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'state': 'starting', 'base': 'aa9bcc5651164ade3265e124de9a6d4f778f561f',
           'scope': 'native source/base/config verification, unchanged observer, core and retained owning carrier'}
    def save():
        RECORD.write_text(json.dumps(row, indent=2) + '\n')
    save()
    try:
        for stage, script in [('core', 'native-token-query-epoch2-gates.ps1'),
                              ('owning', 'assist-native-token-query-epoch2-owned.ps1')]:
            args = ['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', str(SHARE / script)]
            with (SHARE / ('token-query-epoch2-' + stage + '-driver.log')).open('wb') as output:
                child = subprocess.Popen(args, stdout=output, stderr=subprocess.STDOUT)
                row.update({'state': 'running-' + stage, 'child-pid': child.pid, 'argv': args})
                save()
                code = child.wait()
            row[stage + '-exit-code'] = code
            save()
            require(code == 0, 'native ' + stage + ' driver did not complete: ' + str(code))
            if stage == 'core':
                verified = verify_core()
                row['full-library'] = verified['summaries']['full']
                save()
            else:
                archives = load('native-token-query-epoch2-owning-archive-hashes.json')
                require(len(archives['sources']) == 313 and archives['core-features'] == [],
                        'actual owning inputs and features')
                require(all(x['structural-member-walk'] == 'pass' and x['members'] > 200
                            for x in archives['archives']), 'actual complete owning archive walks')
        row.update({'state': 'finished', 'exit-code': 0})
    except BaseException:
        row.update({'state': 'failed', 'exit-code': 1, 'error': traceback.format_exc()})
    finally:
        row['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        save()
        LOCK.unlink()
    print(json.dumps(row, indent=2), flush=True)
    return row['exit-code']


if __name__ == '__main__':
    sys.exit(main())
