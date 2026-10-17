"""Wait for existing drivers, then collect evidence without starting a build."""
from pathlib import Path
import json, subprocess, sys, time

B = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
deadline = time.monotonic() + 7200
while True:
    if time.monotonic() > deadline:
        raise TimeoutError('existing native/Unix validation is still incomplete')
    try:
        native = json.loads((S / 'token-query-epoch2-launch.json').read_text())
        if native.get('state') == 'failed':
            raise RuntimeError(native)
        ready = native.get('state') == 'finished' and native.get('exit-code') == 0
        for host in ['macos', 'linux']:
            for prefix, count in [('token-final', 7), ('token-epoch2', 4)]:
                gates = json.loads((B / (host + '-' + prefix + '-gates.json')).read_text())
                ready &= len(gates) == count and all(x['exit-code'] == 0 for x in gates)
        if ready:
            break
    except (FileNotFoundError, json.JSONDecodeError):
        pass
    time.sleep(10)

p = S / 'copy-completed-token-artifacts.py'
script = '''from pathlib import Path
import json, shutil
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
b=Path('C:/Windows/Temp/assist-native-rax-20261010')
r=json.loads((s/'token-query-epoch2-launch.json').read_text())
if r.get('state')!='finished' or r.get('exit-code')!=0:raise RuntimeError('native driver incomplete')
for name in ['native-token-query-final-build-user-artifacts.jsonl','native-token-query-epoch2-after-artifacts.jsonl','native-token-query-epoch2-after-artifacts-stderr.log']:
 shutil.copyfile(b/name,s/name)
 print(name)
'''
p.write_text(script)
argv = ['prlctl', 'exec', 'a5b14f9a-d292-4610-a04d-df4ca4337c9d',
        r'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe', '-B',
        r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\copy-completed-token-artifacts.py']
r = subprocess.run(argv, capture_output=True, text=True, timeout=120)
(B / 'completed-artifact-copy.json').write_text(json.dumps({'argv': argv, 'exit-code': r.returncode,
                                                          'stdout': r.stdout, 'stderr': r.stderr}, indent=2) + '\n')
print(r.stdout, r.stderr, flush=True)
if r.returncode:
    raise RuntimeError('completed artifact copy failed')
subprocess.run([sys.executable, '-B', str(B / 'collect-token-validation.py')], check=True)
subprocess.run([sys.executable, '-B', str(B / 'rax-final-source/docs/specifications/windows/native-token-query/validation-check.py')], check=True)
subprocess.run([sys.executable, '-B', str(B / 'stress-token-validation.py')], check=True)
print('Completed collection, final verification and adversarial evidence checks', flush=True)
