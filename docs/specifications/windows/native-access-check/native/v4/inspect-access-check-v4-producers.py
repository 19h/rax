"""Independently bind the four native token-query producers and all raw rows."""
from pathlib import Path
import hashlib
import json
import re
import struct
import sys

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
SOURCE = 'be1f2679b56b118a42e5278d28d0fd83cdf601e95b969f50b34a522d00710d19'


def require(ok, text):
    if not ok:
        raise ValueError(text)


def identity(path):
    data = path.read_bytes()
    return {'path': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


def main():
    require(__debug__, 'optimized validation rejected')
    launch = json.loads((SHARE / 'access-check-v4-native-launch.json').read_text())
    require(launch.get('exit-code') == 0, 'native controller completed')
    outcomes = json.loads((SHARE / 'native-access-check-v4-outcomes.json').read_text(encoding='utf-8-sig'))
    profiles = ['arm64', 'x64', 'x86', 'x86-laa']
    require([x['profile'] for x in outcomes] == profiles, 'four profiles')
    records = []
    for profile, outcome in zip(profiles, outcomes):
        require(outcome['exit-code'] == 0, 'producer completed ' + profile)
        folder = BASE / ('access-check-v4-' + profile)
        source = folder / 'native-access-check-v4.cpp'
        stem = 'native-access-check-v4-' + profile
        exe, log = folder / (stem + '.exe'), folder / (stem + '.log')
        require(identity(source)['sha256'] == SOURCE == outcome['source-hash'].lower(), 'compiled source')
        data = exe.read_bytes()
        pe = struct.unpack_from('<I', data, 60)[0]
        require(data[:2] == b'MZ' and data[pe:pe + 4] == b'PE\0\0', 'actual PE')
        machine = struct.unpack_from('<H', data, pe + 4)[0]
        laa = bool(struct.unpack_from('<H', data, pe + 22)[0] & 0x20)
        require(machine == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile],
                'actual native producer machine')
        require(laa == (profile != 'x86'), 'actual large-address-aware header')
        require(identity(exe)['sha256'] == outcome['exe-sha256'].lower() and exe.stat().st_size == outcome['exe-bytes'],
                'actual producer identity')
        text = log.read_text()
        require(text == (SHARE / log.name).read_text(), 'raw stdout copy')
        header = f'identity profile={profile} pointer-bytes={4 if machine == 0x14C else 8} build=10.0.29683 app-container=0 cases=4410'
        require(text.splitlines()[0] == header, 'actual profile header')
        rows = [dict(re.findall(r'([\w-]+)=([^\s]*)', line)) for line in text.splitlines() if line.startswith('case ')]
        require([int(x['ordinal']) for x in rows] == list(range(4410)), 'all ordered calls')
        require(all(x['escaped'] == '00000000' and x['last-error'] == '7E11ABCD' for x in rows),
                'no escaping exception or fabricated Win32 last-error change')
        require(all(len(x['privileges']) == 128 for x in rows), '64-byte privilege backing snapshots')
        require(text.endswith('complete cases=4410 closed-handles=3 released-descriptors=6\n'),
                'complete owned cleanup')
        records.append({'profile': profile, 'machine': machine, 'large-address-aware': laa,
                        'source': identity(source), 'executable': identity(exe), 'stdout': identity(log),
                        'calls': len(rows), 'escaped-exceptions': 0, 'closed-handles': 3, 'released-descriptors': 6})
    result = {'scope': 'four producer ABIs on the same ARM64 Windows10.0.29683 kernel; '
                       'all controlled subjects are non-AppContainer primary and explicit impersonation tokens; no guest implementation proof',
              'producers': records, 'calls': 17640, 'closed-handles': 12, 'released-descriptors': 24}
    output = SHARE / 'native-access-check-v4-identities.json'
    require(not output.exists(), 'do not overwrite independent native evidence')
    output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'verified-producers': 4, 'calls': 17640, 'closed-handles': 12, 'released-descriptors': 24}))


if __name__ == '__main__':
    main()
