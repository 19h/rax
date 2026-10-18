from pathlib import Path
import datetime
import json
import os
import subprocess

b = Path('C:/Windows/Temp/assist-native-rax-20261010')
s = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = s / 'session-final-owned-launch.json'
lock = b / 'session-final-owned.lock'
if record.exists():
    raise RuntimeError('token owning driver already launched')
descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.close(descriptor)
state = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'state': 'running'}
def save():
    record.write_text(json.dumps(state, indent=2) + '\n')
save()
try:
    args = ['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
            str(s / 'assist-native-session-final-owned.ps1')]
    with (s / 'session-final-owned-driver.log').open('wb') as output:
        child = subprocess.Popen(args, stdout=output, stderr=subprocess.STDOUT)
        state.update({'child-pid': child.pid, 'argv': args})
        save()
        state['exit-code'] = child.wait()
    state['state'] = 'finished' if state['exit-code'] == 0 else 'failed'
finally:
    state['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
    lock.unlink()
print(json.dumps(state), flush=True)
raise SystemExit(state['exit-code'])
