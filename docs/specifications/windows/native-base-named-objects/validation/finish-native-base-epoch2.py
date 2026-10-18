from pathlib import Path
import datetime
import json
import os
import subprocess
import sys
import time

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
lock = B / 'base-final-completion-epoch2.lock'
if (S / 'base-final-post-run-completion.json').exists():
    raise RuntimeError('post-run completion already exists')
fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.close(fd)


def run(args, log):
    with (S / log).open('wb') as output:
        return subprocess.run(args, stdout=output, stderr=subprocess.STDOUT).returncode


try:
    owner = S / 'base-owner-observer-launch.json'
    if owner.exists():
        deadline = time.monotonic() + 900
        while True:
            try:
                state = json.loads(owner.read_text())
            except json.JSONDecodeError:
                state = {'state': 'writing'}
            if state['state'] in ['finished', 'failed']:
                if state['state'] != 'finished' or state.get('exit_code') != 0:
                    raise RuntimeError('prestarted owner observer failed')
                break
            if time.monotonic() >= deadline:
                raise RuntimeError('owner observer did not complete')
            time.sleep(1)
    else:
        code = run([sys.executable, '-B', str(S / 'run-base-owner-observer.py')],
                   'base-owner-observer-transport-epoch2.log')
        if code:
            raise RuntimeError('owner observer failed: ' + str(code))
    code = run(['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                str(S / 'partial-native-focus.ps1')], 'partial-native-focus.log')
    if code:
        raise RuntimeError('native namespace focus failed: ' + str(code))
    checks = []
    for script, log in [('verify-base-final.py', 'native-base-final-source-post-run.log'),
                        ('verify-base-post-run.py', 'native-base-final-post-run-verifier.log')]:
        code = run([sys.executable, '-B', str(S / script)], log)
        checks.append({'script': script, 'exit-code': code})
        if code:
            raise RuntimeError('post-run verification failed: ' + script)
    (S / 'base-final-post-run-completion.json').write_text(json.dumps({
        'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'source-epoch': 2, 'checks': checks}, indent=2) + '\n')
finally:
    lock.unlink()
print('Native namespace epoch2 completion checks passed', flush=True)
