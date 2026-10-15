#!/usr/bin/env python3
"""Validate copied native source/output bindings and complete control partitions."""
from pathlib import Path, PureWindowsPath
import hashlib
import json
import re

ROOT = Path(__file__).resolve().parent


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    data = path.read_bytes()
    return len(data), hashlib.sha256(data).hexdigest()


def main():
    require(__debug__, 'optimized corpus validation is rejected')
    require(digest(ROOT / 'ordinary-observer.rs.txt') == (11178, '514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0'), 'unchanged ordinary observer')
    identity = json.loads((ROOT / 'native-shared-section-final-identities.json').read_text())
    require(len(identity['producers']) == 17, 'actual producer count')
    expected = {
        'windows-shared-section-grants2': (32, 4, 6),
        'windows-shared-section-geometry': (40, 20, 1),
        'windows-shared-section-protect': (22, 22, 1),
        'windows-shared-section-flags': (39, 6, 1),
    }
    profiles = {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}
    seen = set()
    total_cases = total_views = total_handles = 0
    for producer in identity['producers']:
        key = producer['group'], producer['profile']
        require(key not in seen, 'duplicate producer partition')
        seen.add(key)
        for field in ['source', 'stdout']:
            row = producer[field]
            path = ROOT / PureWindowsPath(row['path']).name
            require(digest(path) == (row['bytes'], row['sha256']), 'copied native ' + field)
        binary = producer['executable']
        require(binary['machine'] == profiles[producer['profile']], 'actual PE machine')
        require(bool(binary['characteristics'] & 0x20) == (producer['profile'] != 'x86'), 'actual LAA bit')
        raw = (ROOT / PureWindowsPath(producer['stdout']['path']).name).read_text()
        require('failure=' not in raw, 'native control failure')
        if producer['group'] == 'original-first-map':
            require(raw.splitlines()[-1] == 'complete captured=1 calls=3 exit=41', 'original native child exit')
            entries = [line for line in raw.splitlines() if line.startswith('entry ') and 'target=1 ' in line]
            require(len(entries) == 1, 'original target count')
            arguments = entries[0].split('args=', 1)[1].rstrip(',').split(',')
            require(len(arguments) == 10 and int(arguments[8], 16) & 0xFFFFFFFF == 0x00500000, 'original ULONG allocation flags')
            require('section-name characters=22 text=\\Windows\\SharedSection' in raw, 'original object name')
            continue
        cases, views, handles = expected[producer['group']]
        require(len(re.findall(r'^case=', raw, re.M)) == cases, 'complete native case partition')
        require(len(re.findall(r'^unmap status=00000000$', raw, re.M)) == views, 'owned view release')
        require(len(re.findall(r'^close status=00000000$', raw, re.M)) == handles, 'owned handle release')
        require(raw.splitlines()[-1].startswith(f'complete cases={cases} mapped-views={views}'), 'native footer')
        if producer['group'].endswith('flags'):
            rows = [dict(field.split('=', 1) for field in line.split()) for line in raw.splitlines() if line.startswith('case=')]
            require({int(row['flags'], 16) for row in rows} >= {0, *(1 << i for i in range(32)), 0x00500000}, 'one-hot/startup flag coverage')
            for row in rows:
                accepted = int(row['flags'], 16) & ~0x00500001 == 0
                require(int(row['status'], 16) == (0 if accepted else 0xC000000D), 'native accepted flag partition')
        total_cases += cases
        total_views += views
        total_handles += handles
    require(seen == {(group, profile) for group in expected for profile in profiles} | {('original-first-map', 'arm64')}, 'exact producer set')
    require((total_cases, total_views, total_handles) == (532, 208, 36), 'aggregate native lifecycle counts')
    print('Verified17 native producers,532 controls,208 released views,36 closed handles; original allocation flags and exit41')


if __name__ == '__main__':
    main()
