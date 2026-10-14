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
    raise RuntimeError('optimized inspection rejected')

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


expected = '128136641cb933adb866dd911317cb77b3d72103989cf774a8c88197f145c221'
producers = []
for group in ['control', 'extended-control']:
    outcomes = json.loads((SHARE / ('native-process-tls-' + group + '-outcomes.json')).read_text(encoding='utf-8-sig'))
    for profile in ['arm64', 'x64', 'x86', 'x86-laa']:
        directory = TASK / ('process-tls-' + group + '-' + profile)
        stem = 'native-process-tls-' + group + '-' + profile
        source = identity(directory / ('native-process-tls-' + group + '.cpp'))
        executable = identity(directory / (stem + '.exe'), True)
        stdout = identity(directory / (stem + '.log'))
        outcome = next(row for row in outcomes if row['profile'] == profile)
        require(source['sha256'] == expected and source['bytes'] == 8830, 'actual first control source')
        require(outcome['exit-code'] == 0 and outcome['source-hash'].lower() == source['sha256'], 'executed source and exit')
        require(outcome['exe-sha256'].lower() == executable['sha256'] and outcome['exe-bytes'] == executable['bytes'], 'executed binary')
        require(executable['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile], 'machine')
        require(bool(executable['characteristics'] & 0x20) == (profile != 'x86'), 'LAA')
        text = (directory / (stem + '.log')).read_text()
        results = [dict(re.findall(r'([\w-]+)=([^\s]+)', line)) for line in text.splitlines() if line.startswith('result ')]
        exits = [dict(re.findall(r'([\w-]+)=([^\s]+)', line)) for line in text.splitlines() if line.startswith('child-exit ')]
        require([int(row['case']) for row in results] == list(range(44)), '44 ordered results')
        require([int(row['case']) for row in exits] == list(range(44)), '44 ordered owned exits')
        require(all(row['code'] == '77' for row in exits), 'owned collector child completion')
        require(all(row['escaped'] == '00000000' for row in results), 'no escaped native exceptions')
        require('failure=' not in text and text.splitlines()[-1] == f'complete cases=44 child-failures=0 width={4 if profile.startswith("x86") else 8}', 'complete footer')
        producers.append({'group': group, 'profile': profile, 'source': source, 'executable': executable, 'stdout': stdout, 'cases': 44,
                          'role': 'accepted initial controls' if group == 'control' else 'duplicate initial collector after generator failure; excluded from new control count'})

first = []
for profile in ['arm64', 'x64', 'x86']:
    directory = TASK / ('process-tls-first-call-' + profile)
    stem = 'native-process-tls-first-call-' + profile
    source = identity(directory / 'native-process-tls-first-call.cpp')
    executable = identity(directory / (stem + '.exe'), True)
    require(source['sha256'] == 'ce1d5e911b06e5ace3ba0a022042d5b9209a03df18646d76ddc07ab69bb15567', 'actual debugger source')
    text = (directory / (stem + '.log')).read_text()
    require('complete captured=1 calls=5 exit=41' in text if profile == 'arm64' else 'complete captured=0' in text, 'debugger scope result')
    first.append({'profile': profile, 'source': source, 'executable': executable, 'stdout': identity(directory / (stem + '.log')),
                  'captured-native-calls': 1 if profile == 'arm64' else 0})

observer_source = identity(TASK / 'native-process-tls-input-trace.rs')
require(observer_source['sha256'] == 'c3082490d046b5d8b189aebca474faead90594ebced79f25a01ce308d433bdba', 'actual observer source')
observer_executable = identity(TASK / 'native-process-tls-input-trace.exe', True)
require(observer_executable['machine'] == 0xAA64, 'observer executable machine')
native = ctypes.c_ushort()
process = ctypes.c_ushort()
query = ctypes.windll.kernel32.IsWow64Process2
query.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ushort), ctypes.POINTER(ctypes.c_ushort)]
require(query(ctypes.c_void_p(-1), ctypes.byref(process), ctypes.byref(native)) != 0 and native.value == 0xAA64 and process.value == 0, 'native inspector architecture')
libraries = [identity(Path('C:/Windows') / folder / name, True) for folder in ['System32', 'SysWOW64'] for name in ['ntdll.dll', 'kernel32.dll']]
baseline = json.loads((TASK / 'src/docs/specifications/windows/native-performance-counter/native-performance-counter-identities.json').read_text())
require(libraries == baseline['libraries'], 'unchanged physical library identities')
result = {'captured-utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'kernel': list(sys.getwindowsversion()),
          'principal': subprocess.check_output(['whoami', '/user', '/fo', 'csv', '/nh'], text=True).strip(),
          'native-machine': native.value, 'process-machine': process.value, 'libraries': libraries,
          'inspector': identity(Path(__file__)), 'producers': producers, 'first-call-captures': first,
          'observer': {'source': observer_source, 'executable': observer_executable, 'stdout': identity(TASK / 'native-process-tls-input-trace-output.log')},
          'accepted-control-count': 176, 'excluded-duplicate-control-count': 176}
(SHARE / 'native-process-tls-initial-identities.json').write_text(json.dumps(result, indent=2) + '\n')
print('Accepted 176 initial controls; 176 duplicate controls explicitly excluded; one native debugger capture.')
