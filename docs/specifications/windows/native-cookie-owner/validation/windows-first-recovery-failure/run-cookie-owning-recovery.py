from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import sys
import traceback

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
RECORD = SHARE / 'cookie-owner-final-owning-recovery.json'
LOCK = BASE / 'cookie-owner-final-owning-recovery.lock'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def main():
    require(__debug__, 'optimized recovery rejected')
    previous = json.loads((SHARE / 'cookie-owner-final-launch.json').read_text())
    require(previous.get('core-exit-code') == 0 and previous.get('owning-exit-code') == 101
            and previous.get('exit-code') == 1, 'actual completed initial owning failure')
    require(not RECORD.exists(), 'recovery already requested; inspect actual result')
    fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    os.close(fd)
    row = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'scope': 'restore missing task-owned junction and rerun owning checks only; core evidence unchanged'}
    def save():
        RECORD.write_text(json.dumps(row, indent=2) + '\n')
    save()
    try:
        link = BASE / 'assist/vendor/rax'
        source = BASE / 'src'
        require(not os.path.lexists(link) and (source / 'capi/Cargo.toml').is_file(),
                'missing task-owned link and existing validated source')
        subprocess.run(['cmd.exe', '/d', '/c', 'mklink', '/J', str(link), str(source)], check=True)
        require(link.resolve() == source.resolve(), 'junction resolves to validated native source')
        manifest = (link / 'capi/Cargo.toml').read_bytes()
        row.update({'junction-target': os.readlink(link),
                    'capi-manifest-sha256': hashlib.sha256(manifest).hexdigest()})
        script = SHARE / 'assist-native-cookie-owner-final-recovered-owned.ps1'
        with (SHARE / 'cookie-owner-final-recovery-driver.log').open('wb') as log:
            argv = ['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', str(script)]
            child = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT)
            row.update({'state': 'running', 'child-pid': child.pid, 'argv': argv})
            save()
            row['exit-code'] = child.wait()
        require(row['exit-code'] == 0, 'recovered owning checks failed')
        archive = json.loads((SHARE / 'native-cookie-owner-final-recovered-owning-archive-hashes.json').read_text())
        require(len(archive['sources']) == 310 and archive['core-features'] == [], 'actual source/feature binding')
        require(len(archive['archives']) == 2 and all(x['structural-member-walk'] == 'pass'
                                                  for x in archive['archives']), 'complete owning archives')
        row['state'] = 'finished'
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
