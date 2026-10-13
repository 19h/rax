#!/usr/bin/env python3
"""Reject controlled changes after rebinding stdout, without mutating evidence."""
import argparse
from contextlib import redirect_stdout
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import tempfile


def main(directory):
    original = Path(directory)
    spec = importlib.util.spec_from_file_location('scheduler_evidence', original / 'check-evidence.py')
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    with redirect_stdout(io.StringIO()):
        checker.main(original)
    controls = [
        ('compiled-source', 'actual compiled source binding'),
        ('pe-machine', 'actual PE machine'),
        ('x86-laa', 'actual PE LAA'),
        ('missing-producer', 'independent producer inventory'),
        ('native-dll', 'captured installed native DLL identities'),
        ('input', 'independent input partition differs'),
        ('missing-row', 'independent input partition cardinality'),
        ('status', 'native constructor status precedence'),
        ('birth', 'native constructor birth count'),
        ('grant', 'native typed handle metadata'),
        ('restore', 'native exact handle restoration'),
        ('guard', 'native guard protection ordering'),
        ('bytes', 'native exact constructor bytes'),
        ('fresh-entry', 'fresh original-entry/true-LR cardinality'),
        ('fresh-svc', 'fresh live MEM_IMAGE SVC'),
        ('fresh-class', 'fresh startup constructor partition'),
        ('fresh-thread', 'fresh thread/function/true-LR binding'),
        ('fresh-publication', 'fresh actual handle publication'),
        ('fresh-grant', 'fresh actual typed handle ownership'),
        ('fresh-lifetime', 'fresh capture lifetime'),
        ('manifest', 'complete evidence file inventory'),
    ]
    results = []
    with tempfile.TemporaryDirectory(prefix='rax-scheduler-evidence-') as temporary:
        root = Path(temporary) / 'corpus'
        shutil.copytree(original, root, ignore=shutil.ignore_patterns('__pycache__'))
        originals = {}

        def write(name, raw):
            path = root / name
            originals.setdefault(name, path.read_bytes())
            path.write_bytes(raw)

        def store(name, data):
            write(name, (json.dumps(data, indent=2) + '\n').encode())

        def change(line, key, value):
            tokens = line.split()
            index = next(i for i, part in enumerate(tokens) if part.startswith(key + '='))
            tokens[index] = key + '=' + value
            return ' '.join(tokens)

        identity_name = 'native-scheduler-shared-producer-identities.json'
        for name, expected in controls:
            identity = checker.read_json(root / identity_name)
            producer = identity['producers'][0]
            if name == 'compiled-source':
                write(producer['source'], (root / producer['source']).read_bytes() + b'\n// changed\n')
            elif name == 'pe-machine':
                producer['executable-identity']['machine'] = 0x14c
                store(identity_name, identity)
            elif name == 'x86-laa':
                producer = next(p for p in identity['producers'] if p['profile'] == 'x86-laa')
                producer['executable-identity']['characteristics'] &= ~0x20
                store(identity_name, identity)
            elif name == 'missing-producer':
                identity['producers'].pop(0)
                store(identity_name, identity)
            elif name == 'native-dll':
                identity['native-libraries'][0]['sha256'] = '0' * 64
                store(identity_name, identity)
            elif name == 'manifest':
                manifest = checker.read_json(root / 'evidence-manifest.json')
                manifest.pop(producer['source'])
                store('evidence-manifest.json', manifest)
            else:
                if name.startswith('fresh-'):
                    producer = next(p for p in identity['producers'] if p['family'] == 'fresh-v2')
                lines = (root / producer['log']).read_text().splitlines()
                if name.startswith('fresh-'):
                    prefix = {'entry': 'entry ', 'svc': 'image ', 'class': 'entry ', 'thread': 'return ',
                              'publication': 'bytes ', 'grant': 'owner ', 'lifetime': 'case='}[name[6:]]
                    index = next(i for i, line in enumerate(lines) if line.startswith(prefix) and
                                 ('warmup=0' in line if name == 'fresh-lifetime' else
                                  'phase=output-after' in line if name == 'fresh-publication' else True))
                    if name == 'fresh-entry':
                        lines.pop(index)
                    elif name == 'fresh-svc':
                        lines[index] = lines[index].replace('bytes=810300D4', 'bytes=010000D4', 1)
                    elif name == 'fresh-class':
                        lines[index] = lines[index].replace(',35,', ',34,', 1)
                    else:
                        key, value = {'fresh-thread': ('thread', '0'), 'fresh-publication': ('data', '0' * 16),
                                      'fresh-grant': ('grant', '00000000'),
                                      'fresh-lifetime': ('exact-handle-set', '0')}[name]
                        lines[index] = change(lines[index], key, value)
                else:
                    index = next(i for i, line in enumerate(lines) if line.startswith('case=') and
                                 ('born-count=1' in line if name == 'grant' else True))
                    if name == 'missing-row':
                        lines.pop(index)
                    else:
                        key, value = {'input': ('process', '1'), 'status': ('status', '00000000'),
                                      'birth': ('born-count', '1'), 'restore': ('restored', '0'),
                                      'guard': ('protection', '00000004,00000104,'),
                                      'bytes': ('bytes0', '00' * 32),
                                      'grant': ('born', checker.fields(lines[index])['born'].replace(
                                          ':00000001:', ':00000000:'))}[name]
                        lines[index] = change(lines[index], key, value)
                raw = ('\n'.join(lines) + '\n').encode()
                write(producer['log'], raw)
                producer['stdout-identity']['bytes'] = len(raw)
                producer['stdout-identity']['sha256'] = hashlib.sha256(raw).hexdigest()
                store(identity_name, identity)
            try:
                with redirect_stdout(io.StringIO()):
                    checker.main(root)
            except ValueError as failure:
                if str(failure) != expected:
                    raise ValueError(name + ': unrelated rejection ' + str(failure)) from failure
                results.append({'control': name, 'result': 'rejected', 'reason': str(failure)})
            else:
                raise ValueError('accepted falsification: ' + name)
            for filename, raw in originals.items():
                (root / filename).write_bytes(raw)
            originals.clear()
    print(json.dumps({'positive': 'pass', 'negative-controls': results}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--corpus', default=str(Path(__file__).parent))
    main(parser.parse_args().corpus)
