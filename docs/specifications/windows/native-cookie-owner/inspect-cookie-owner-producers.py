"""Bind independently read native files, all case records, and the host kernel."""
from pathlib import Path
import ctypes
import hashlib
import json
import re
import struct
import sys

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
SOURCE = '84bd3ec255fa747f84c22150e8254f544379bdbf6bc53ce7d245ddd3593f9a7a'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def identity(path):
    data = path.read_bytes()
    return {'path': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


def pe_identity(path):
    data = path.read_bytes()
    require(data[:2] == b'MZ', 'DOS signature')
    offset = struct.unpack_from('<I', data, 60)[0]
    require(data[offset:offset + 4] == b'PE\0\0', 'PE signature')
    return {'machine': struct.unpack_from('<H', data, offset + 4)[0],
            'large-address-aware': bool(struct.unpack_from('<H', data, offset + 22)[0] & 0x20),
            'major-subsystem-version': struct.unpack_from('<H', data, offset + 24 + 48)[0]}


def main():
    require(__debug__, 'optimized verification rejected')
    launch = json.loads((SHARE / 'cookie-owner-native-launch.json').read_text())
    require(launch.get('exit-code') == 0, 'completed native oracle driver')
    outcomes = json.loads((SHARE / 'native-cookie-owner-outcomes.json').read_text(encoding='utf-8-sig'))
    profiles = ['arm64', 'x64', 'x86', 'x86-laa']
    require([item['profile'] for item in outcomes] == profiles, 'four native profiles')
    rows = []
    for profile, outcome in zip(profiles, outcomes):
        require(outcome['exit-code'] == 0, 'native producer completed: ' + profile)
        folder = BASE / ('cookie-owner-' + profile)
        source = folder / 'native-cookie-owner.cpp'
        stem = 'native-cookie-owner-' + profile
        exe = folder / (stem + '.exe')
        log = folder / (stem + '.log')
        require(identity(source)['sha256'] == SOURCE == outcome['source-hash'].lower(), 'actual compiled source')
        actual = pe_identity(exe)
        require(actual['machine'] == {'arm64': 0xAA64, 'x64': 0x8664, 'x86': 0x14C, 'x86-laa': 0x14C}[profile],
                'actual producer machine')
        require(actual['large-address-aware'] == (profile != 'x86') and actual['major-subsystem-version'] == 10,
                'actual producer header conditions')
        require(identity(exe)['sha256'] == outcome['exe-sha256'].lower() and exe.stat().st_size == outcome['exe-bytes'],
                'actual producer executable identity')
        text = log.read_text()
        require(text == (SHARE / log.name).read_text(), 'same raw native/shared stdout')
        lines = text.splitlines()
        require(len(lines) == 4 and all(line.startswith('case ') and line.endswith('cleanup=pass') for line in lines),
                'exact four case records and all cleanup')
        cases = []
        for line in lines:
            row = dict(re.findall(r'([\w-]+)=([^\s\[\]]+)', line))
            child = re.search(r' child=\[([^\]]*)\]', line)
            require(child is not None and row['profile'] == profile, 'child record and profile')
            row['child-record'] = child.group(1)
            require(int(row['pointer-bytes']) == (4 if actual['machine'] == 0x14C else 8), 'actual pointer width')
            require(row['wait'] == '00000000', 'no timeout can establish a startup result')
            cases.append(row)
        require([row['name'] for row in cases] == ['untouched', 'restore-default', 'preseed', 'zero'], 'case selection')
        rows.append({'profile': profile, 'source': identity(source), 'executable': identity(exe),
                     'pe': actual, 'stdout': identity(log), 'cases': cases})
    class Version(ctypes.Structure):
        _fields_ = [('size', ctypes.c_ulong), ('major', ctypes.c_ulong), ('minor', ctypes.c_ulong),
                    ('build', ctypes.c_ulong), ('platform', ctypes.c_ulong), ('service', ctypes.c_wchar * 128)]
    version = Version()
    version.size = ctypes.sizeof(version)
    rtl = ctypes.WinDLL('ntdll')
    rtl.RtlGetVersion.argtypes = [ctypes.POINTER(Version)]
    rtl.RtlGetVersion.restype = ctypes.c_long
    require(rtl.RtlGetVersion(ctypes.byref(version)) == 0, 'actual kernel version')
    system = ctypes.create_string_buffer(64)
    ctypes.WinDLL('kernel32').GetNativeSystemInfo(ctypes.byref(system))
    processor = struct.unpack_from('<H', system.raw)[0]
    result = {'scope': 'actual native suspended task-owned children on one ARM64 kernel; '
                       'counterfactual mutations are not emulator success results',
              'kernel': {'major': version.major, 'minor': version.minor, 'build': version.build,
                         'native-processor-architecture': processor},
              'inspector-python': identity(Path(sys.executable)),
              'inspector-python-pe': pe_identity(Path(sys.executable)),
              'producers': rows, 'children': 16, 'closed-child-handle-pairs': 16}
    require(processor == 12 and result['inspector-python-pe']['machine'] == 0xAA64, 'native ARM64 inspector')
    path = SHARE / 'native-cookie-owner-identities.json'
    require(not path.exists(), 'do not replace earlier native oracle identities')
    path.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'verified-profiles': 4, 'children': 16, 'kernel': result['kernel']}))


if __name__ == '__main__':
    main()
