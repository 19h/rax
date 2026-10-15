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



directory=TASK/'section-create-first-call-arm64';stem='native-section-create-first-call-arm64'
source=identity(directory/'native-section-create-first-call.cpp');exe=identity(directory/(stem+'.exe'),True);stdout=identity(directory/(stem+'.log'));outcome=json.loads((SHARE/'native-section-create-first-call-outcomes.json').read_text(encoding='utf-8-sig'))
if source['sha256']!='82a8537ef0465dc1f09c093ff471fe8dc81635a6d7dc8748eebca49dce3f0e23' or source['sha256']!=identity(SHARE/'native-section-create-first-call.cpp')['sha256'] or source['sha256']!=outcome['source-hash'].lower():raise ValueError('actual source')
if exe['machine']!=0xAA64 or exe['sha256']!=outcome['exe-sha256'].lower() or exe['bytes']!=outcome['exe-bytes'] or outcome['exit-code']!=0:raise ValueError('actual executable')
if stdout['sha256']!=identity(SHARE/(stem+'.log'))['sha256']:raise ValueError('actual stdout')
t=(directory/(stem+'.log')).read_text()
if not re.search(r'^entry ordinal=0 target=1 .*args=[0-9A-F]+,F001F,0,[0-9A-F]+,4,8000000,0,$',t,re.M) or 'return ordinal=0 class=0 status=00000000' not in t or 'hex=0000010000000000' not in t or 'created-section-query status=00000000 returned=24 hex=000000000000000000000008000000000000010000000000' not in t or t.splitlines()[-1]!='complete captured=1 calls=1 exit=41':raise ValueError('original7args and natural native exit')
row={'captured-utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':list(sys.getwindowsversion()),'source':source,'executable':exe,'stdout':stdout,'inspector':identity(Path(__file__)),'max-size-bytes':65536,'protection':4,'allocation-attributes':0x08000000,'desired-access':0xF001F,'original-native-child-exit':41,'scope':'native debugger child only; no ordinary emulated process exit'}
(SHARE/'native-section-create-first-call-identities.json').write_text(json.dumps(row,indent=2)+'\n');print('Verified actual native NtCreateSection original7args,65536 bytes,PAGE_READWRITE,SEC_COMMIT and native child exit41')
