from pathlib import Path
import ctypes
import datetime
import hashlib
import json
import struct


S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
destination = S / 'native-timer2-set-svc-map.json'
assert not destination.exists()
module_path = Path('C:/Windows/System32/ntdll.dll')
before = hashlib.sha256(module_path.read_bytes()).hexdigest()
assert before == '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
module = ctypes.WinDLL('ntdll')
rows = []
for name in ('NtCreateTimer2', 'NtSetTimer2', 'NtCancelTimer2', 'NtQueryTimer', 'NtWaitForSingleObject'):
    function = getattr(module, name)
    address = ctypes.cast(function, ctypes.c_void_p).value
    raw = ctypes.string_at(address, 16)
    found = [(offset, (word >> 5) & 0xFFFF)
             for offset, (word,) in zip(range(0, 16, 4), struct.iter_unpack('<I', raw))
             if word & 0xFFE0001F == 0xD4000001]
    assert len(found) == 1, (name, raw.hex(), found)
    rows.append({'name': name, 'address': hex(address), 'first-16-bytes': raw.hex(),
                 'svc-offset': found[0][0], 'svc': found[0][1]})
assert next(r['svc'] for r in rows if r['name'] == 'NtSetTimer2') == 0x1c0
assert next(r['svc'] for r in rows if r['name'] == 'NtWaitForSingleObject') == 4
assert hashlib.sha256(module_path.read_bytes()).hexdigest() == before
record = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'module-path': str(module_path),
          'module-sha256': before, 'rows': rows,
          'source-sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'scope': 'Read-only installed ARM64 NTDLL function bytes; diagnostic labels only, no guest dispatch changes.'}
destination.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
