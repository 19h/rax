import hashlib
import json
import pathlib
import struct

root = pathlib.Path(r'C:\Windows\Temp\assist-native-rax-20261010')
rows = []
for family, profile in [('basic', p) for p in ('arm64', 'x64', 'x86', 'x86-laa')] + [('default-stack', p) for p in ('arm64', 'x86')]:
    base = root / ('worker-factory-' + family + '-probe-' + profile)
    path = base / ('native-worker-factory-' + family + '-probe-' + profile + '.exe')
    data = path.read_bytes()
    nt = struct.unpack_from('<I', data, 0x3c)[0]
    machine = struct.unpack_from('<H', data, nt + 4)[0]
    flags = struct.unpack_from('<H', data, nt + 22)[0]
    opt = nt + 24
    magic = struct.unpack_from('<H', data, opt)[0]
    width = 8 if magic == 0x20b else 4
    # SizeOfStackReserve is at optional-header byte72 in PE32 and PE32+.
    # PE32+ byte88 is SizeOfHeapReserve; the earlier collector confused them.
    off = 72
    reserve, commit = struct.unpack_from('<QQ' if width == 8 else '<II', data, opt + off)
    source = base / ('native-worker-factory-' + family + '-probe.cpp')
    rows.append({'family': family, 'profile': profile, 'path': str(path), 'bytes': len(data),
                 'sha256': hashlib.sha256(data).hexdigest(), 'machine': hex(machine),
                 'LAA': bool(flags & 0x20), 'PE-stack-reserve': reserve,
                 'PE-stack-commit': commit,
                 'source-sha256': hashlib.sha256(source.read_bytes()).hexdigest()})
output = pathlib.Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\worker-factory-default-stack-identity.json')
output.write_text(json.dumps(rows, indent=2) + '\n', encoding='utf-8')
print(output.read_text())
