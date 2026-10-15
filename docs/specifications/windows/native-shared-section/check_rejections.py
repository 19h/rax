#!/usr/bin/env python3
"""Require independent corpus validation to reject controlled metadata/data edits."""
from pathlib import Path, PureWindowsPath
import copy
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
NAME = 'native-shared-section-final-identities.json'


def main():
    if not __debug__:
        raise RuntimeError('optimized rejection checks are rejected')
    original = json.loads((ROOT / NAME).read_text())
    count = 0
    with tempfile.TemporaryDirectory(prefix='rax-shared-section-rejections-') as temporary:
        base = Path(temporary)
        shutil.copytree(ROOT, base / 'corpus')
        corpus = base / 'corpus'
        for mutation in ['machine', 'laa', 'duplicate', 'source_hash', 'release', 'startup_flags']:
            record = copy.deepcopy(original)
            producer = record['producers'][0]
            changed_file = None
            if mutation == 'machine':
                producer['executable']['machine'] = 0
            elif mutation == 'laa':
                producer['executable']['characteristics'] ^= 0x20
            elif mutation == 'duplicate':
                record['producers'][-1] = copy.deepcopy(producer)
            elif mutation == 'source_hash':
                producer['source']['sha256'] = '0' * 64
            else:
                if mutation == 'startup_flags':
                    producer = next(p for p in record['producers'] if p['group'] == 'original-first-map')
                row = producer['stdout']
                changed_file = corpus / PureWindowsPath(row['path']).name
                raw = changed_file.read_bytes()
                if mutation == 'release':
                    modified = raw.replace(b'unmap status=00000000', b'unmap status=C0000022', 1)
                else:
                    # Change only the actual target's raw ULONG argument; rehash
                    # so the semantic check, rather than byte identity, rejects it.
                    lines = raw.splitlines(keepends=True)
                    for index, line in enumerate(lines):
                        if line.startswith(b'entry ') and b'target=1 ' in line:
                            before, arguments = line.split(b'args=', 1)
                            values = arguments.rstrip(b'\r\n').split(b',')
                            values[8] = b'0'
                            ending = b'\r\n' if line.endswith(b'\r\n') else b'\n'
                            lines[index] = before + b'args=' + b','.join(values) + ending
                    modified = b''.join(lines)
                if modified == raw:
                    raise ValueError('controlled mutation did not alter input')
                changed_file.write_bytes(modified)
                row['bytes'] = len(modified)
                row['sha256'] = hashlib.sha256(modified).hexdigest()
            (corpus / NAME).write_text(json.dumps(record))
            result = subprocess.run([sys.executable, '-B', str(corpus / 'check.py')], capture_output=True)
            if result.returncode == 0:
                raise ValueError('accepted controlled mutation: ' + mutation)
            if changed_file is not None:
                shutil.copy2(ROOT / changed_file.name, changed_file)
            count += 1
    print(f'Rejected all {count} controlled source, identity, lifecycle and startup-flag mutations')


if __name__ == '__main__':
    main()
