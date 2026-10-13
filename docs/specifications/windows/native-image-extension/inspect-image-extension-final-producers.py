from pathlib import Path
import hashlib
import json
import struct
import subprocess
import sys

task = Path(r'C:\Windows\Temp\assist-native-rax-20261010')
share = Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')

def identity(path, pe=False):
    data = path.read_bytes()
    result = {'path': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    if pe:
        assert data[:2] == b'MZ'
        offset = struct.unpack_from('<I', data, 0x3c)[0]
        assert data[offset:offset + 4] == b'PE\0\0'
        result['machine'] = struct.unpack_from('<H', data, offset + 4)[0]
        result['characteristics'] = struct.unpack_from('<H', data, offset + 22)[0]
    return result

producers = []
for family in ['query-probe', 'boundaries-v2', 'limits', 'fresh']:
    stem = 'native-image-extension-' + family
    profiles = ['arm64'] if family == 'fresh' else ['arm64', 'x64', 'x86', 'x86-laa']
    for profile in profiles:
        directory = task / ('image-extension-' + profile)
        log = directory / (stem + '-' + profile + '.log')
        source = directory / (stem + '.cpp')
        executable = directory / (stem + '-' + profile + '.exe')
        text = log.read_text()
        rows = sum(line.startswith('case=') for line in text.splitlines())
        expected = 3 if family == 'fresh' else 4200 if family == 'query-probe' else 1092 if family == 'limits' else 15480
        assert rows == expected
        assert ('complete cases=2 warmup=1' if family == 'fresh' else 'complete cases=' + str(expected)) in text
        producers.append({'family': family, 'profile': profile,
                          'source': source.name, 'log': log.name,
                          'source-identity': identity(source),
                          'executable-identity': identity(executable, True),
                          'stdout-identity': identity(log), 'rows': rows})

result = {'kernel': list(sys.getwindowsversion()),
          'principal': subprocess.check_output(['whoami', '/user', '/fo', 'csv', '/nh'], text=True).strip(),
          'python': sys.version, 'inspector-identity': identity(Path(__file__)),
          'native-libraries': [identity(Path(r'C:\Windows\System32\ntdll.dll'), True),
                               identity(Path(r'C:\Windows\SysWOW64\ntdll.dll'), True)],
          'producers': producers}
(share / 'native-image-extension-producer-identities.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'actual-producers': len(producers), 'mature-rows': sum(p['rows'] for p in producers if p['family'] != 'fresh'),
                  'fresh-cases': 2, 'principal': result['principal']}, indent=2))
