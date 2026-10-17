"""Verify the retained native cookie oracle without claiming fixed-core success."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).resolve().parent


def require(ok, message):
    if not ok:
        raise ValueError(message)


def load(name):
    return json.loads((ROOT / name).read_text(encoding='utf-8-sig'))


def main():
    require(__debug__, 'optimized verification rejected')
    manifest = load('evidence.json')
    seen = set()
    for row in manifest['files']:
        name = row['path']
        require(not Path(name).is_absolute() and '..' not in Path(name).parts and name not in seen,
                'unique relative evidence path')
        seen.add(name)
        data = (ROOT / name).read_bytes()
        require((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']), name)
    rax = ROOT.parents[3]
    inputs = load('final-code-inputs.json')
    require(inputs['base'] == '8b038b528d8cd88e44e5ebd672ef14e365c480a5' and len(inputs['files']) == 2,
            'published parent and two frozen code inputs')
    for item in inputs['files']:
        data = (rax / item['path']).read_bytes()
        require((len(data), hashlib.sha256(data).hexdigest()) == (item['bytes'], item['sha256']),
                'current frozen source ' + item['path'])
    identities = load('native-cookie-owner-identities.json')
    require(identities['kernel'] == {'major': 10, 'minor': 0, 'build': 29683,
                                    'native-processor-architecture': 12}, 'measured ARM64 kernel')
    require(identities['children'] == identities['closed-child-handle-pairs'] == 16, 'sixteen closed children')
    profiles = ['arm64', 'x64', 'x86', 'x86-laa']
    require([x['profile'] for x in identities['producers']] == profiles, 'four actual producer profiles')
    for producer in identities['producers']:
        profile = producer['profile']
        source = (ROOT / 'native-cookie-owner.cpp').read_bytes()
        require(hashlib.sha256(source).hexdigest() == producer['source']['sha256'], 'actual compiled source')
        log = ROOT / ('native-cookie-owner-' + profile + '.log')
        require(hashlib.sha256(log.read_bytes()).hexdigest() == producer['stdout']['sha256'], 'native stdout')
        cases = producer['cases']
        require([x['name'] for x in cases] == ['untouched', 'restore-default', 'preseed', 'zero'], 'case identities')
        default = 0xBB40E64E if profile.startswith('x86') else 0x2B992DDFA232
        for case in cases:
            require(int(case['file-cookie'], 16) == int(case['suspended-cookie'], 16) == default,
                    'file-default cookie before first child instruction')
            require(case['cleanup'] == 'pass' and case['wait'] == '00000000', 'no timeout or cleanup failure')
            starts = case['name'] in ['untouched', 'restore-default']
            require(int(case['exit'], 16) == (0 if starts else 0xC000007B), 'actual counterfactual startup status')
            require(int(case['reported']) == int(starts), 'only completed child entry reports its cookie')
            if starts:
                cookie = re.search(r'cookie=([0-9A-F]+)', case['child-record'])
                require(cookie is not None and int(cookie.group(1), 16) not in [0, default],
                        'native startup initializes the cookie after suspended creation')
    snapshot = load('suspended-native-cookie-state.json')
    require(snapshot['first-thread-resumed'] is False and snapshot['cleanup'] == 'pass',
            'independent suspended-only read with cleanup')
    require(len(snapshot['modules']) == 2 and all(x['suspended-cookie'] == x['file']['cookie'] == 0x2B992DDFA232
                                                for x in snapshot['modules']),
            'both executable and native NTDLL retain file-default cookies before initial thread')
    print('Verified16 native child results, four producer identities, independent executable/NTDLL '
          'suspended state and byte-bound initial loader diagnosis; fixed-core validation is separate')


if __name__ == '__main__':
    main()
