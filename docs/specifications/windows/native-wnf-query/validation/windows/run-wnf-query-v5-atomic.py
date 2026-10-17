from pathlib import Path
import datetime
import json
import os
import subprocess
import sys

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = SHARE / 'wnf-query-v5-native-launch.json'
lock = BASE / 'wnf-query-v5-native-driver.lock'
if record.exists():
    raise RuntimeError('Token query oracle already requested; inspect before any retry')
descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.write(descriptor, str(os.getpid()).encode())
os.close(descriptor)
row = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
       'lock': str(lock), 'scope': 'native NtQueryWnfStateData on private temporary process states and controlled read-only names; no RAX source mutation'}
try:
    with (SHARE / 'wnf-query-v5-native-driver.log').open('wb') as output:
        args = ['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                str(SHARE / 'native-wnf-query-v5-profiles.ps1')]
        child = subprocess.Popen(args, stdout=output, stderr=subprocess.STDOUT)
        row.update({'child-pid': child.pid, 'argv': args})
        record.write_text(json.dumps(row, indent=2) + '\n')
        row['exit-code'] = child.wait()
finally:
    row['completed-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    record.write_text(json.dumps(row, indent=2) + '\n')
    lock.unlink()
print(json.dumps(row, indent=2), flush=True)
sys.exit(row['exit-code'])
