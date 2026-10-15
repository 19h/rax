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



rows=[];outcomes=json.loads((SHARE/'native-section-create-valid-cross-outcomes.json').read_text(encoding='utf-8-sig'))
for profile in ['arm64','x64','x86','x86-laa']:
 directory=TASK/('section-create-valid-cross-'+profile);stem='native-section-create-valid-cross-'+profile;source=identity(directory/'native-section-create-valid-cross.cpp');exe=identity(directory/(stem+'.exe'),True);stdout=identity(directory/(stem+'.log'));outcome=next(x for x in outcomes if x['profile']==profile)
 if source['sha256']!='fc5d1041830dd48d8890dfc047eead35cdb36f4b387e463a394863b3faff6d21' or source['sha256']!=identity(SHARE/'native-section-create-valid-cross.cpp')['sha256'] or source['sha256']!=outcome['source-hash'].lower():raise ValueError('actual source')
 if exe['machine']!={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[profile] or bool(exe['characteristics']&0x20)!=(profile!='x86') or exe['sha256']!=outcome['exe-sha256'].lower() or exe['bytes']!=outcome['exe-bytes'] or outcome['exit-code']!=0:raise ValueError('actual executable')
 if stdout['sha256']!=identity(SHARE/(stem+'.log'))['sha256']:raise ValueError('actual stdout')
 t=(directory/(stem+'.log')).read_text()
 if 'failure=' in t or len(re.findall(r'^case=',t,re.M))!=1344 or len(re.findall(r'^close status=00000000$',t,re.M))!=288 or t.splitlines()[-1]!='complete cases=1344 closed-handles=288':raise ValueError('counts or lifetime')
 rows.append({'profile':profile,'source':source,'executable':exe,'stdout':stdout,'controls':1344,'closed-handles':288})
record={'captured-utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':list(sys.getwindowsversion()),'producers':rows,'inspector':identity(Path(__file__)),'scope':'5376 direct creation controls and1152 released handles on ARM64 kernel; x64/x86 are compatibility modes'}
(SHARE/'native-section-create-valid-cross-identities.json').write_text(json.dumps(record,indent=2)+'\n');print('Verified4 actual native producers,5376 creation controls,1152 closed handles')
