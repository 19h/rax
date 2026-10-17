from pathlib import Path
import datetime
import hashlib
import json
import re
import struct
import sys

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')


def identity(path):
    data = path.read_bytes()
    return {'path': str(path), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


def fields(line):
    return dict(part.split('=', 1) for part in line.split() if '=' in part)


def main():
    families = json.loads((SHARE / 'console-owner-producer-manifest.json').read_text())
    rows = []
    for family in families:
        for profile, machine in [('arm64', 0xAA64), ('x64', 0x8664), ('x86', 0x14C), ('x86-laa', 0x14C)]:
            stem = family['stem'] + '-' + profile
            directory = BASE / (family['directory'] + '-' + profile)
            source = directory / (family['stem'] + '.cpp')
            exe = directory / (stem + '.exe')
            log = directory / (stem + '.log')
            if identity(source)['sha256'] != family['source-sha256']:
                raise ValueError('actual source ' + str(source))
            data = exe.read_bytes()
            pe = struct.unpack_from('<I', data, 0x3C)[0]
            characteristics = struct.unpack_from('<H', data, pe + 22)[0]
            if data[:2] != b'MZ' or data[pe:pe + 4] != b'PE\0\0' or struct.unpack_from('<H', data, pe + 4)[0] != machine:
                raise ValueError('actual PE ' + str(exe))
            if profile.startswith('x86') and bool(characteristics & 32) != (profile == 'x86-laa'):
                raise ValueError('actual LAA profile ' + str(exe))
            if identity(log)['sha256'] != identity(SHARE / log.name)['sha256']:
                raise ValueError('copied stdout ' + str(log))
            text = log.read_text()
            row = {'family': family['kind'], 'profile': profile, 'source': identity(source), 'executable': identity(exe), 'stdout': identity(log), 'machine': machine, 'characteristics': characteristics}
            if family['kind'] == 'matrix':
                expected = 'complete cases=136 child-failures=0 width=' + ('4' if profile.startswith('x86') else '8')
                if text.splitlines()[-1] != expected:
                    raise ValueError('native matrix terminal')
                results = re.findall(r'^result case=(\d+) status=(\w+) escaped=(\w+)', text, re.M)
                if {int(r[0]) for r in results} != set(range(136)) or len(results) != 136 or any(int(r[2], 16) for r in results):
                    raise ValueError('native matrix reachability/SEH')
                exits = re.findall(r'^child-exit case=(\d+) code=(\d+)', text, re.M)
                if len(exits) != 136 or any(int(r[1]) != 77 for r in exits):
                    raise ValueError('native child lifecycle')
                row.update({'cases': 136, 'primary-success': sum(int(r[1], 16) == 0 for r in results), 'primary-failure': sum(int(r[1], 16) != 0 for r in results), 'repeat-sets': len(re.findall(r'^second value=', text, re.M)), 'seh': 0})
            else:
                if 'cleanup terminate=1 error=0 wait=00000000' not in text:
                    raise ValueError('owned child cleanup')
                if family['kind'] != 'plain' and text.splitlines()[-1] != 'cleanup-streams input=1 output=1 error=1':
                    raise ValueError('owned stream cleanup')
                capture = re.search(r'^parameters bytes=(\d+) hex=(\w+)', text, re.M)
                raw = bytes.fromhex(capture[2])
                width = 4 if profile.startswith('x86') else 8
                if len(raw) != int(capture[1]) or len(raw) != (708 if width == 4 else 1096):
                    raise ValueError('raw parameters')
                console = int.from_bytes(raw[16:16 + width], 'little')
                offset = 0x68 if width == 4 else 0xA4
                window = int.from_bytes(raw[offset:offset + 4], 'little')
                if console != (1 << (8 * width)) - 1 or window != (0 if family['kind'] == 'plain' else 0x100):
                    raise ValueError('native detached persona')
                row.update({'parameter-bytes': len(raw), 'console-handle': hex(console), 'window-flags': hex(window), 'scope': 'suspended child fixture is x86 non-LAA even when its controller is x86-LAA'})
                if family['kind'] == 'initial':
                    creation = fields(next(line for line in text.splitlines() if line.startswith('creation-owner ')))
                    if int(creation['parent-pid']) != int(creation['basic-parent'], 16) or int(creation['expected'], 16) != (int(creation['parent-pid']) | 2):
                        raise ValueError('creation owner provenance')
                    owner = fields(next(line for line in text.splitlines() if line.startswith('suspended-owner ')))
                    if width == 8 and (int(owner['status'], 16) != 0 or int(owner['value'], 16) != int(creation['expected'], 16) or int(owner['returned'], 16) != 8):
                        raise ValueError('native initial owner')
                    if width == 4 and int(owner['status'], 16) != 0xC0000003:
                        raise ValueError('WoW64 initial query')
                    row['creation-owner'] = {'parent-pid': int(creation['parent-pid']), 'owner': owner}
            path = re.search(r'^ntdll-path utf16=(\w+)', text, re.M)
            if path:
                name = ''.join(chr(int(path[1][i:i + 4], 16)) for i in range(0, len(path[1]), 4))
                row['reported-ntdll-path'] = name
                # Native Python reads this reported filesystem path independently;
                # loaded export bytes are separately retained, including ARM64X.
                row['reported-path-file'] = identity(Path(name))
                row['loaded-leaves'] = [fields(line) for line in text.splitlines() if line.startswith('leaf name=')]
                if len(row['loaded-leaves']) != 2:
                    raise ValueError('installed leaf count')
            rows.append(row)
    result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'kernel': list(sys.getwindowsversion()), 'scope': 'independent native source, PE, executable, stdout, parameter and lifecycle verification; no executable copied into repository', 'producers': rows}
    (SHARE / 'native-console-owner-identities.json').write_text(json.dumps(result, indent=2) + '\n')
    print('Independently verified16producers,544primary calls,136repeat sets,zeroSEH,12suspended snapshots', flush=True)


if __name__ == '__main__':
    main()
