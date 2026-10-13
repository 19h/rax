#!/usr/bin/env python3
"""Controlled falsification with exact rejection reasons and immutable inputs."""
from contextlib import redirect_stdout
import argparse
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import tempfile


def main(directory):
    original = Path(directory)
    spec = importlib.util.spec_from_file_location('evidence', original / 'check-evidence.py')
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    with redirect_stdout(io.StringIO()):
        checker.main(original)
    identity_name = 'native-image-extension-final-producer-identities.json'
    results = []
    controls = [
        ('changed-compiled-source', 'actual compiled source binding'),
        ('forged-pe-machine', 'actual PE machine'),
        ('forged-x86-laa', 'actual PE LAA'),
        ('missing-producer', 'independent producer inventory'),
        ('changed-native-dll', 'captured installed native DLL identities'),
        ('changed-rehashed-input', 'independent input partition differs'),
        ('missing-rehashed-row', 'independent input partition cardinality'),
        ('changed-rehashed-status', 'same-width native semantic agreement'),
        ('changed-rehashed-guard-bytes', 'same-width native semantic agreement'),
        ('changed-rehashed-handle-restoration', 'native query handle restoration'),
        ('changed-rehashed-fresh-lifetime', 'fresh capture lifetime'),
        ('missing-rehashed-fresh-entry', 'fresh original-entry/true-LR cardinality'),
        ('changed-rehashed-fresh-svc', 'fresh live MEM_IMAGE SVC'),
        ('changed-rehashed-fresh-thread', 'fresh thread/function/true-LR binding'),
        ('changed-rehashed-fresh-query-class', 'fresh startup query partition'),
        ('changed-rehashed-fresh-payload', 'fresh actual extension payload'),
        ('missing-manifest-file', 'complete evidence file inventory'),
    ]
    with tempfile.TemporaryDirectory(prefix='rax-image-extension-evidence-') as temporary:
        root = Path(temporary) / 'corpus'
        shutil.copytree(original, root, ignore=shutil.ignore_patterns('__pycache__'))
        originals = {}

        def write(name, data):
            path = root / name
            originals.setdefault(name, path.read_bytes())
            path.write_bytes(data)

        def store(name, data):
            write(name, (json.dumps(data, indent=2) + '\n').encode())

        def change_field(line, field, replacement):
            tokens = line.split()
            index = next(i for i, token in enumerate(tokens) if token.startswith(field + '='))
            tokens[index] = field + '=' + replacement
            return ' '.join(tokens)

        for name, expected_failure in controls:
            identity = checker.read_json(root / identity_name)
            producer = identity['producers'][0]
            if name == 'changed-compiled-source':
                write(producer['source'], (root / producer['source']).read_bytes() + b'\n// falsification\n')
            elif name == 'forged-pe-machine':
                producer['executable-identity']['machine'] = 0x14c
                store(identity_name, identity)
            elif name == 'forged-x86-laa':
                producer = next(item for item in identity['producers'] if item['profile'] == 'x86-laa')
                producer['executable-identity']['characteristics'] &= ~0x20
                store(identity_name, identity)
            elif name == 'missing-producer':
                identity['producers'].pop(0)
                store(identity_name, identity)
            elif name == 'changed-native-dll':
                identity['native-libraries'][0]['sha256'] = '0' * 64
                store(identity_name, identity)
            elif name == 'missing-manifest-file':
                manifest = checker.read_json(root / 'evidence-manifest.json')
                manifest.pop(producer['source'])
                store('evidence-manifest.json', manifest)
            else:
                if 'fresh' in name:
                    producer = next(item for item in identity['producers'] if item['family'] == 'fresh')
                elif name in ['changed-rehashed-guard-bytes', 'changed-rehashed-handle-restoration']:
                    producer = next(item for item in identity['producers'] if
                                    item['family'] == 'boundaries-v2' and item['profile'] == 'arm64')
                lines = (root / producer['log']).read_text().splitlines()
                index = next(i for i, line in enumerate(lines) if line.startswith('case='))
                if name == 'missing-rehashed-row':
                    lines.pop(index)
                elif name == 'changed-rehashed-input':
                    lines[index] = change_field(lines[index], 'target', '1')
                elif name == 'changed-rehashed-status':
                    value = checker.fields(lines[index])['status']
                    lines[index] = change_field(lines[index], 'status', format(int(value, 16) ^ 1, '08X'))
                elif name == 'changed-rehashed-guard-bytes':
                    value = checker.fields(lines[index])['bytes0']
                    lines[index] = change_field(lines[index], 'bytes0', format(int(value[:2], 16) ^ 1, '02X') + value[2:])
                elif name == 'changed-rehashed-handle-restoration':
                    index = next(i for i, line in enumerate(lines) if line.startswith('complete '))
                    lines[index] = change_field(lines[index], 'handles-after', '0')
                elif name == 'changed-rehashed-fresh-lifetime':
                    index = next(i for i, line in enumerate(lines) if line.startswith('case=') and 'warmup=0' in line)
                    lines[index] = change_field(lines[index], 'exact-handle-set', '0')
                elif name == 'missing-rehashed-fresh-entry':
                    lines = [line for line in lines if not (line.startswith('entry ') and ',E,' in line)]
                elif name == 'changed-rehashed-fresh-svc':
                    index = next(i for i, line in enumerate(lines) if line.startswith('image '))
                    lines[index] = lines[index].replace('bytes=610400D4', 'bytes=010000D4', 1)
                elif name == 'changed-rehashed-fresh-thread':
                    index = next(i for i, line in enumerate(lines) if line.startswith('return ') and 'call=2 ' in line)
                    lines[index] = change_field(lines[index], 'thread', '0')
                elif name == 'changed-rehashed-fresh-query-class':
                    index = next(i for i, line in enumerate(lines) if line.startswith('entry ') and 'call=0 ' in line)
                    lines[index] = lines[index].replace(',6,', ',5,', 1)
                elif name == 'changed-rehashed-fresh-payload':
                    index = next(i for i, line in enumerate(lines) if line.startswith('bytes ') and 'phase=output-after ' in line)
                    value = checker.fields(lines[index])['data']
                    lines[index] = change_field(lines[index], 'data', '01' + value[2:])
                data = ('\n'.join(lines) + '\n').encode()
                write(producer['log'], data)
                producer['stdout-identity']['bytes'] = len(data)
                producer['stdout-identity']['sha256'] = hashlib.sha256(data).hexdigest()
                store(identity_name, identity)
            try:
                with redirect_stdout(io.StringIO()):
                    checker.main(root)
            except ValueError as failure:
                if expected_failure != str(failure):
                    raise ValueError(name + ' rejected for an unrelated reason: ' + str(failure)) from failure
                results.append({'control': name, 'result': 'rejected', 'reason': str(failure)})
            else:
                raise ValueError('accepted falsified evidence: ' + name)
            for filename, data in originals.items():
                (root / filename).write_bytes(data)
            originals.clear()
    print(json.dumps({'positive': 'pass', 'negative-controls': results}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--corpus', default=str(Path(__file__).parent))
    main(parser.parse_args().corpus)
