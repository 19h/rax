from pathlib import Path
import ctypes
import datetime
import hashlib
import json
import re
import struct
import subprocess
import sys

if not __debug__:
    raise RuntimeError('optimized native inspection rejected')

TASK = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def identity(path, executable=False):
    data = path.read_bytes()
    row = {'path': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    if executable:
        require(data[:2] == b'MZ', 'DOS signature')
        offset = struct.unpack_from('<I', data, 0x3C)[0]
        require(data[offset:offset + 4] == b'PE\0\0', 'PE signature')
        row['machine'] = struct.unpack_from('<H', data, offset + 4)[0]
        row['characteristics'] = struct.unpack_from('<H', data, offset + 22)[0]
    return row


initial = json.loads((SHARE / 'native-process-tls-initial-identities.json').read_text())
outcomes = json.loads((SHARE / 'native-process-tls-alias-control-outcomes.json').read_text(encoding='utf-8-sig'))
producers = []
for profile in ['arm64', 'x64', 'x86', 'x86-laa']:
    directory = TASK / ('process-tls-alias-control-' + profile)
    stem = 'native-process-tls-alias-control-' + profile
    source = identity(directory / 'native-process-tls-alias-control.cpp')
    executable = identity(directory / (stem + '.exe'), True)
    stdout = identity(directory / (stem + '.log'))
    require(source['sha256'] == '7a937428810bc1b1ebac06844bb7373d4a84736e4406e2e2e3eff74061e8b2a9' and source['bytes'] == 16695, 'actual alias source')
    outcome = next(row for row in outcomes if row['profile'] == profile)
    require(outcome['exit-code'] == 0 and outcome['source-hash'].lower() == source['sha256'], 'executed source and exit')
    require(outcome['exe-bytes'] == executable['bytes'] and outcome['exe-sha256'].lower() == executable['sha256'], 'executed binary')
    require(executable['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile], 'machine')
    require(bool(executable['characteristics'] & 0x20) == (profile != 'x86'), 'LAA')
    text = (directory / (stem + '.log')).read_text()
    results = [dict(re.findall(r'([\w-]+)=([^\s]+)', line)) for line in text.splitlines() if line.startswith('result ')]
    exits = [dict(re.findall(r'([\w-]+)=([^\s]+)', line)) for line in text.splitlines() if line.startswith('child-exit ')]
    require([int(row['case']) for row in results] == list(range(146, 180)), '34 ordered results')
    require([int(row['case']) for row in exits] == list(range(146, 180)) and all(row['code'] == '77' for row in exits), '34 owned child completions')
    require(all(row['escaped'] == '00000000' for row in results), 'no escaped call exceptions')
    require('failure=' not in text and text.splitlines()[-1] == f'complete cases=34 child-failures=0 width={4 if profile.startswith("x86") else 8}', 'complete footer')
    producers.append({'profile': profile, 'source': source, 'executable': executable, 'stdout': stdout, 'cases': 34})

libraries = [identity(Path(row['path']), True) for row in initial['libraries']]
require(libraries == initial['libraries'], 'unchanged physical libraries')
native = ctypes.c_ushort()
process = ctypes.c_ushort()
query = ctypes.windll.kernel32.IsWow64Process2
query.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ushort), ctypes.POINTER(ctypes.c_ushort)]
require(query(ctypes.c_void_p(-1), ctypes.byref(process), ctypes.byref(native)) != 0 and native.value == 0xAA64 and process.value == 0, 'native inspector architecture')
require(list(sys.getwindowsversion()) == initial['kernel'], 'same physical kernel')
result = {'captured-utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'kernel': initial['kernel'],
          'principal': subprocess.check_output(['whoami', '/user', '/fo', 'csv', '/nh'], text=True).strip(), 'native-machine': native.value, 'process-machine': process.value,
          'libraries': libraries, 'inspector': identity(Path(__file__)), 'producers': producers,
          'initial-identities': identity(SHARE / 'native-process-tls-initial-identities.json'),
          'accepted-initial-control-count': 176, 'accepted-extended-control-count': 408,
          'accepted-alias-control-count': 136, 'accepted-total-control-count': 720,
          'excluded-duplicate-control-count': 176, 'native-debugger-captures': 1}
(SHARE / 'native-process-tls-alias-identities.json').write_text(json.dumps(result, indent=2) + '\n')
print('Verified 136 alias controls; total accepted 720; duplicate 176 excluded.')
