from pathlib import Path
import datetime
import hashlib
import json
import subprocess
import time

B = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
manifest = S / 'native-base-final-build-user-artifacts.jsonl'
record = S / 'namespace-churn-new-run.json'
if record.exists():
    raise RuntimeError('revised churn controller already recorded')
row = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
       'state': 'waiting-for-epoch2-core-artifact', 'source-epoch': 2}


def save():
    record.write_text(json.dumps(row, indent=2) + '\n')


save()
deadline = time.monotonic() + 1800
while time.monotonic() < deadline:
    if manifest.exists() and manifest.stat().st_mtime > 1792322315:
        messages = []
        for line in manifest.read_text(encoding='utf-8-sig').splitlines():
            try:
                messages.append(json.loads(line))
            except json.JSONDecodeError:
                pass
        if messages and messages[-1].get('reason') == 'build-finished':
            if not messages[-1]['success']:
                raise RuntimeError('current core build failed')
            selected = [m for m in messages if m.get('reason') == 'compiler-artifact'
                        and m.get('target', {}).get('name') == 'rax']
            if not selected:
                raise RuntimeError('no Cargo-selected core archive')
            row.update({'state': 'running', 'cargo-artifact': selected[-1],
                        'artifact-stream-sha256': hashlib.sha256(manifest.read_bytes()).hexdigest()})
            break
    time.sleep(5)
else:
    row.update(state='artifact-wait-expired')
    save()
    raise RuntimeError('current native archive was not ready within 1800 s')
args = ['prlctl', 'exec', 'a5b14f9a-d292-4610-a04d-df4ca4337c9d',
        'powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        'C:\\Mac\\Home\\Downloads\\assist-native-allocate-ex-transfer-20261010-2154\\namespace-churn-new.ps1']
row['argv'] = args
save()
with (B / 'namespace-churn-new-transport.log').open('wb') as log:
    result = subprocess.run(args, stdout=log, stderr=subprocess.STDOUT)
row.update({'state': 'finished', 'exit-code': result.returncode,
            'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
save()
print('Revised native churn completed:', result.returncode, flush=True)
raise SystemExit(result.returncode)
