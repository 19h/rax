"""Preserve executed records and add explicitly traced FileSystem artifact names."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil
import time


F = Path(__file__).resolve().parent
record = F / 'filesystem-unix-record-promotion.json'
assert not record.exists()
state = {'state': 'waiting', 'pid': os.getpid(), 'hosts': [],
         'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
         'scope': 'The executed Unix driver retained a previous-group log prefix. Preserve its original record and log bytes; add traced FileSystem filename aliases without changing commands, results, hashes, or test selection.'}


def save():
    record.write_text(json.dumps(state, indent=2) + '\n')


save()
deadline = time.monotonic() + 3600
for host in ['macos', 'linux']:
    original = F / ('policy-' + host + '-epoch1-gates.json')
    while True:
        raw = original.read_bytes()
        data = json.loads(raw)
        assert data['state'] != 'failed', original
        if data['state'] == 'finished':
            assert data['exit-code'] == 0
            break
        assert time.monotonic() < deadline
        time.sleep(3)
    mapping = []
    for row in data['steps']:
        source = F / row['log']
        before = source.read_bytes()
        assert len(before) == row['bytes'] and hashlib.sha256(before).hexdigest() == row['sha256']
        target = F / row['log'].replace('policy-', 'filesystem-', 1)
        assert not target.exists()
        shutil.copy2(source, target)
        assert target.read_bytes() == before
        mapping.append({'original': row['log'], 'alias': target.name,
                        'bytes': len(before), 'sha256': row['sha256']})
        row['log'] = target.name
    data['artifact-name-provenance'] = {'original-record': original.name,
        'original-record-bytes': len(raw), 'original-record-sha256': hashlib.sha256(raw).hexdigest(),
        'scope': 'Only log basename aliases changed; original executed argv, results, exit codes and bound log bytes are unchanged.'}
    target = F / ('filesystem-' + host + '-epoch1-gates.json')
    assert not target.exists()
    target.write_text(json.dumps(data, indent=2) + '\n')
    state['hosts'].append({'host': host, 'original-record': original.name,
                           'canonical-record': target.name, 'log-mapping': mapping})
    save()
state.update({'state': 'finished', 'exit-code': 0,
              'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
save()
print(json.dumps(state), flush=True)
