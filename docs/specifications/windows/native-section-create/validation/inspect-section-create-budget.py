from pathlib import Path
import datetime
import hashlib
import json
import re
import struct
import sys

TASK = Path("C:/Windows/Temp/assist-native-rax-20261010")
SHARE = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")


def identity(path, executable=False):
    data = path.read_bytes()
    row = {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    if executable:
        if data[:2] != b"MZ":
            raise ValueError("DOS signature")
        pe = struct.unpack_from("<I", data, 0x3C)[0]
        if data[pe:pe + 4] != b"PE\0\0":
            raise ValueError("PE signature")
        row["machine"] = struct.unpack_from("<H", data, pe + 4)[0]
        row["characteristics"] = struct.unpack_from("<H", data, pe + 22)[0]
    return row



root=TASK/'assist';source=identity(root/'native-section-create-budget-probe.cpp');exe=identity(root/'native-cpp/native-section-create-budget-probe.exe',True);stdout=identity(TASK/'native-section-create-budget-output.log');outcome=json.loads((SHARE/'native-section-create-budget-outcome.json').read_text(encoding='utf-8-sig'))
if source['sha256']!='a20d74618d052932fdf0914f17832474871c8103b433030a2ce83429aa759398' or source['sha256']!=outcome['source-sha256'].lower() or source['sha256']!=identity(SHARE/'native-section-create-budget-probe.cpp')['sha256']:raise ValueError('actual source')
if exe['machine']!=0xAA64 or exe['sha256']!=outcome['exe-sha256'].lower() or exe['bytes']!=outcome['exe-bytes'] or outcome['exit-code']!=0:raise ValueError('actual executable')
if stdout['sha256']!=identity(SHARE/'native-section-create-budget-output.log')['sha256']:raise ValueError('actual stdout')
text=(TASK/'native-section-create-budget-output.log').read_text();runs=[line for line in text.splitlines() if line.startswith('run reason=')]
if len(runs)!=4 or any(not line.startswith('run reason=5 exit=0 diagnostic=') for line in runs) or sum('NtSecureConnectPort (0x194)' in line for line in runs)!=3 or 'NtTraceEvent (0x5e)' not in runs[-1]:raise ValueError('actual result classification')
observer=identity(TASK/'native-shared-section-trace.rs')
if observer['sha256']!='514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0':raise ValueError('unchanged observer')
record={'source':source,'executable':exe,'stdout':stdout,'observer':observer,'inspector':identity(Path(__file__)),'scope':'additional60s C++ diagnostic; three explicit SecureConnectPort failures and whoami TraceEvent; ordinary guest exits0'}
(SHARE/'native-section-create-budget-identities.json').write_text(json.dumps(record,indent=2)+'\n');print('Verified actual ARM64 budget probe and unchanged11178B observer; three SecureConnectPort failures,no ordinary exits')
