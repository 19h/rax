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



producers=[]
for group,cases,maps,closes in [("windows-shared-section-grants2",32,4,6),("windows-shared-section-geometry",40,20,1),("windows-shared-section-protect",22,22,1)]:
    outcomes=json.loads((SHARE/f"native-{group}-outcomes.json").read_text(encoding="utf-8-sig"))
    for profile in ["arm64","x64","x86","x86-laa"]:
        directory=TASK/f"{group}-{profile}";stem=f"native-{group}-{profile}"
        source=identity(directory/f"native-{group}.cpp")
        exe=identity(directory/f"{stem}.exe",True)
        stdout=identity(directory/f"{stem}.log")
        outcome=next(o for o in outcomes if o["profile"]==profile)
        if outcome["exit-code"]!=0 or outcome["source-hash"].lower()!=source["sha256"] or source["sha256"]!=identity(SHARE/f"native-{group}.cpp")["sha256"]: raise ValueError("actual source/exit")
        if exe["sha256"]!=outcome["exe-sha256"].lower() or exe["bytes"]!=outcome["exe-bytes"]:raise ValueError("actual executable")
        if exe["machine"]!={"arm64":0xAA64,"x64":0x8664,"x86":0x14C,"x86-laa":0x14C}[profile] or bool(exe["characteristics"]&0x20)!=(profile!="x86"):raise ValueError("actual architecture/LAA")
        if stdout["sha256"]!=identity(SHARE/f"{stem}.log")["sha256"]:raise ValueError("transfer stdout")
        text=(directory/f"{stem}.log").read_text()
        if "failure=" in text or len(re.findall(r"^case=",text,re.M))!=cases or len(re.findall(r"^unmap status=00000000$",text,re.M))!=maps or len(re.findall(r"^close status=00000000$",text,re.M))!=closes:raise ValueError("native count/lifetime")
        footer=f"complete cases={cases} mapped-views={maps}"
        if not text.splitlines()[-1].startswith(footer):raise ValueError("native footer")
        if group.endswith("grants2") and len(re.findall(r"^grant status=00000000 .*returned=56$",text,re.M))!=6:raise ValueError("exact basic ABI")
        producers.append({"group":group,"profile":profile,"source":source,"executable":exe,"stdout":stdout,"cases":cases,"mapped-views":maps,"closed-handles":closes})
directory=TASK/"shared-section-first-map-arm64";stem="native-shared-section-first-map-arm64"
source=identity(directory/"native-shared-section-first-map.cpp");exe=identity(directory/f"{stem}.exe",True);stdout=identity(directory/f"{stem}.log")
outcome=json.loads((SHARE/"native-shared-section-first-map-outcomes.json").read_text(encoding="utf-8-sig"))
if outcome["exit-code"]!=0 or source["sha256"]!=outcome["source-hash"].lower() or exe["sha256"]!=outcome["exe-sha256"].lower() or exe["bytes"]!=outcome["exe-bytes"] or exe["machine"]!=0xAA64:raise ValueError("original producer identity")
text=(directory/f"{stem}.log").read_text()
if source["sha256"]!=identity(SHARE/"native-shared-section-first-map.cpp")["sha256"] or stdout["sha256"]!=identity(SHARE/f"{stem}.log")["sha256"] or text.splitlines()[-1]!="complete captured=1 calls=3 exit=41" or "target=1" not in text:raise ValueError("original map/exit")
producers.append({"group":"original-first-map","profile":"arm64","source":source,"executable":exe,"stdout":stdout,"original-arguments-and-status":True,"natural-child-exit":41})
libraries=[identity(Path("C:/Windows")/folder/name,True) for folder in ["System32","SysWOW64"] for name in ["ntdll.dll","kernel32.dll"]]
parent=json.loads((SHARE/"native-apphelp-identities.json").read_text())
if libraries!=parent["libraries"]:raise ValueError("physical library drift")
output={"captured-utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"kernel":list(sys.getwindowsversion()),"scope":"13 actual producers;376 direct NT controls,184 owned views,32 handle closes;original unmodified-argument startup map and natural exit41","libraries":libraries,"producers":producers,"inspector":identity(Path(__file__))}
(SHARE/"native-shared-section-extended-identities.json").write_text(json.dumps(output,indent=2)+"\n",encoding="utf-8")
print("Verified13 actual producers,376 direct NT controls,184 owned views and32 closed section handles; original child exited41")
