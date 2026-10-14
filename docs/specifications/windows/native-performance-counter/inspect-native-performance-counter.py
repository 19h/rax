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
    result = {'path': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    if executable:
        require(data[:2] == b'MZ', 'DOS signature')
        offset = struct.unpack_from('<I', data, 0x3C)[0]
        require(data[offset:offset + 4] == b'PE\0\0', 'PE signature')
        result['machine'] = struct.unpack_from('<H', data, offset + 4)[0]
        result['characteristics'] = struct.unpack_from('<H', data, offset + 22)[0]
    return result


freeze = json.loads((SHARE / 'performance-counter-source-freeze.json').read_text())
outcomes = json.loads((SHARE / 'native-performance-counter-outcomes.json').read_text(encoding='utf-8-sig'))
producers = []
profiles = ['arm64', 'x64', 'x86', 'x86-laa']
for profile in profiles:
    directory = TASK / ('performance-counter-' + profile)
    stem = 'native-performance-counter-' + profile
    source = identity(directory / 'native-performance-counter.cpp')
    executable = identity(directory / (stem + '.exe'), True)
    output = identity(directory / (stem + '.log'))
    for key in ['bytes', 'sha256']:
        require(source[key] == freeze['native-performance-counter.cpp'][key], 'actual frozen source')
    outcome = next(row for row in outcomes if row['profile'] == profile)
    require(outcome['exit-code'] == 0 and outcome['source-hash'].lower() == source['sha256'], 'executed source and exit')
    require(outcome['exe-sha256'].lower() == executable['sha256'] and outcome['exe-bytes'] == executable['bytes'], 'actual executed binary')
    require(executable['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile], 'actual machine')
    require(bool(executable['characteristics'] & 0x20) == (profile != 'x86'), 'actual LAA')
    text = (directory / (stem + '.log')).read_text()
    rows = [dict(re.findall(r'([\w-]+)=([^\s]+)', line)) for line in text.splitlines() if line.startswith('case=')]
    require(len(rows) == 250 and [int(row['case']) for row in rows] == list(range(250)), 'complete ordered cases')
    require(all(row['escaped'] == '00000000' for row in rows), 'no escaped collector exception')
    require('failure=' not in text and f'complete cases=250 monotonic=8 pointer-bytes={4 if profile.startswith("x86") else 8}' in text, 'complete producer footer')
    calibration = dict(re.findall(r'([\w-]+)=([^\s]+)', text.splitlines()[0]))
    require(calibration['status'] == '00000000' and int(calibration['frequency']) > 0 and calibration['frequency'] == calibration['public-frequency'], 'native/public frequency agreement')
    monotonic = [dict(re.findall(r'([\w-]+)=([^\s]+)', line)) for line in text.splitlines() if line.startswith('monotonic ')]
    require(len(monotonic) == 8 and all(int(row['current']) >= int(row['previous']) and int(row['delta']) == int(row['current']) - int(row['previous']) for row in monotonic), 'monotonic elapsed counters')
    for row in rows[:225]:
        if row['counter-kind'] != '0' or row['frequency-kind'] in ['0', '4', '5']:
            continue
        counter = bytes(int(row.get('b' + str(64 + i), 'A5'), 16) for i in range(8))
        value = int.from_bytes(counter, 'little')
        require(int(row['before']) <= value <= int(row['after']), 'native counter publication precedes optional frequency fault')
    producers.append({'profile': profile, 'source': source, 'executable': executable, 'stdout': output, 'calibration': calibration, 'cases': 250, 'monotonic-observations': 8})

native = ctypes.c_ushort()
process = ctypes.c_ushort()
query = ctypes.windll.kernel32.IsWow64Process2
query.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ushort), ctypes.POINTER(ctypes.c_ushort)]
require(query(ctypes.c_void_p(-1), ctypes.byref(process), ctypes.byref(native)) != 0 and native.value == 0xAA64 and process.value == 0, 'native inspector architecture')
result = {
    'captured-utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'kernel': list(sys.getwindowsversion()),
    'principal': subprocess.check_output(['whoami', '/user', '/fo', 'csv', '/nh'], text=True).strip(),
    'native-machine': native.value,
    'process-machine': process.value,
    'inspector': identity(Path(__file__)),
    'libraries': [identity(Path('C:/Windows') / folder / name, True) for folder in ['System32', 'SysWOW64'] for name in ['ntdll.dll', 'kernel32.dll']],
    'producers': producers,
}
(SHARE / 'native-performance-counter-identities.json').write_text(json.dumps(result, indent=2) + '\n')
print('actual native producer identities', len(producers), 'matrix cases', sum(row['cases'] for row in producers))
