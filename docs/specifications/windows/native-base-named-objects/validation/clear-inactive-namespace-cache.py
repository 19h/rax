from pathlib import Path
import datetime
import json
import shutil

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record = S / 'namespace-storage-cleanup.json'
if record.exists():
    raise RuntimeError('cleanup already recorded')
state = json.loads((S / 'base-final-launch.json').read_text())
if state['state'] != 'failed' or state['source-epoch'] != 2 or state['core-exit-code'] != 101:
    raise RuntimeError('namespace driver is not the recorded failed, stopped run')
cache = B / 'target-system-time-final/debug/incremental'
if not cache.is_dir() or cache.is_symlink() or cache.is_junction():
    raise RuntimeError('unexpected incremental-cache directory')
size = count = 0
for path in cache.rglob('*'):
    if path.is_symlink() or path.is_junction():
        raise RuntimeError('link in incremental cache')
    if path.is_file():
        size += path.stat().st_size
        count += 1
before = shutil.disk_usage(B)
shutil.rmtree(cache)
after = shutil.disk_usage(B)
row = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
       'deleted-directory': str(cache), 'files': count, 'bytes': size,
       'free-before': before.free, 'free-after': after.free,
       'preserved': 'All source trees, debug/deps archives and executables, logs, other build targets'}
record.write_text(json.dumps(row, indent=2) + '\n')
print(json.dumps(row), flush=True)
