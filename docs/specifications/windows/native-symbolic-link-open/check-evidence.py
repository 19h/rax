#!/usr/bin/env python3
"""Independent input partitions and actual producer/source/PE bindings."""
from pathlib import Path
import argparse
import hashlib
import itertools
import json


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rows(text, prefix="case="):
    return [dict(s.split("=", 1) for s in line.split() if "=" in s)
            for line in text.splitlines() if line.startswith(prefix)]


def partition(family, width):
    # Axes are independent of the emitted case count, manifest and row contents.
    if family == "native-symbolic-link-open-matrix":
        rights = [0,1,2,3,4,8,15,0x10000,0x20000,0x40000,0x80000,0xF0000,
                  0x100000,0x1000000,0x2000000,0x80000000,0x40000000,
                  0x20000000,0x10000000,0xFFFFFFFF]
        output = []
        names = [r"\KnownDlls\KnownDllPath",r"\KnownDlls32\KnownDllPath",
                 r"\knowndlls\knowndllpath","KnownDllPath",r"\KnownDlls","",
                 "\\KnownDlls\\KnownDllPath\\",r"\KnownDlls\\x",
                 r"\KnownDlls\rax-absent-20261012",r"\rax-absent-20261012\x",
                 r"\KnownDlls\ntdll.dll","\\","KnownDlls","ntdll.dll"]
        def one(f,n,a,fl,al,root,mode,length=None,maximum=None):
            natural = len(names[n].encode("utf-16-le"))
            length = natural if length is None else length
            maximum = (length + 2) & 65535 if maximum is None else maximum
            output.append((f,n,a,fl,al,root,mode,length,maximum))
        for n,a,fl in itertools.product(range(14),rights,[0,0x40]):
            one(0,n,a,fl,width * 6,0,0)
        for fl,n in itertools.product([0,1,2,4,8,16,32,64,128,256,512,1024,2048,4096,8192,0xFFFFFFFF],[0,2,8]):
            one(1,n,1,fl,width * 6,0,0)
        for al,mode in itertools.product([0,1,4,23,24,25,47,48,49,0xFFFFFFFF],range(7)):
            one(2,0,1,0x40,al,0,mode)
        for root,n in itertools.product(range(7),[0,3,4,5,8,12,13]):
            one(3,n,1,0x40,width * 6,root,0)
        for length,maximum,mode in itertools.product([0,1,2,23,24,25,45,46,47,48,65530,65532,65534,65535],[0,1,23,24,25,45,46,47,48,65535,None],[0,2,3]):
            one(4,0,1,0x40,width * 6,0,mode,length,maximum)
        return ["family","name","access","flags","attributes-length","root","mode","name-length","maximum"], output
    if family == "native-symbolic-link-open-boundaries":
        return ["attributes","output","unicode","buffer","attributes-length"], list(itertools.product(range(10),range(10),range(5),range(5),[0,width * 6]))
    if family == "native-symbolic-link-open-flag-boundaries":
        output=[(0,o,0,b,width*6,l,f) for f,o,b,l in itertools.product([64,1,32,96,256,0xFFFFFFFF],[0,2,3],range(5),[0,1,46,65534])]
        output += [(0,o,n,0,width*6,46,f) for f,o,n in itertools.product([64,1,32,96,256,0xFFFFFFFF],[0,3],range(5))]
        return ["attributes","output","unicode","buffer","attributes-length","name-length","flags"], output
    if family == "native-symbolic-link-open-name-probe":
        return ["flags","root","output","pattern","length","layout"], list(itertools.product([0,64],range(2),range(4),range(4),[0,1,2,18,20,22,24,46,48,64,65532],range(13)))
    if family == "native-symbolic-link-open-reference":
        return ["parent","source-access","tag","name"], list(itertools.product(range(5),[1,0x20000,0x2000000],range(6),range(8)))
    if family == "native-symbolic-link-open-duplicate-policy":
        return ["parent","source-access","desired","options","source-flags"], list(itertools.product(range(2),[1,0x2000000],[0,1,2,4,8,0x10000,0x20000,0x100000,0x1000000,0x2000000,0x80000000,0x40000000,0x20000000,0x10000000,0xFFFFFFFF],[0,1,2,3,4,5,6,7,8,0xFFFFFFFF],range(4)))
    if family in ["native-symbolic-link-open-duplicate-access","native-symbolic-link-directory-duplicate-access"]:
        return ["parent","source-access","desired","options","source-flags"], list(itertools.product(range(2),[1,0x2000000],[0,1,2,4,8,16,32,64,128,256,512,1024,2048,4096,8192,16384,32768,65535,0x10000,0x100000,0x200000,0x400000,0x800000,0x1000000,0x2000000,0x4000000,0x8000000,0xFFFFFFFF],range(4),[0,3]))
    if family in ["native-symbolic-link-open-duplicate-boundaries","native-symbolic-link-open-duplicate-protected-boundaries"]:
        options = [0,1,2,3] if family.endswith("open-duplicate-boundaries") else [0,1,4,5,6,7]
        return ["desired","options","output-kind"], list(itertools.product([1,4,0x1000000,0xFFFFFFFF],options,range(7)))
    if family == "native-symbolic-link-query-matrix":
        output=[(0,h,l,m,0,0,r) for h,l,m,r in itertools.product(range(11),[0,1,37,38,39,40,65535],[0,1,2,37,38,39,40,41,42,64,65534,65535],[0,1])]
        output += [(1,h,0x1234,m,d,0,r) for h,m,d,r in itertools.product([0,2,3,4,6],[0,38,39,40,64],range(8),[0,1,2])]
        output += [(2,h,0x1234,m,0,b,r) for h,m,b,r in itertools.product([0,2,3,4,6],[0,38,39,40,64],range(8),[0,1,2])]
        output += [(3,h,0x1234,m,0,b,r) for h,m,r,b in itertools.product([0,2,3,4,6],[0,38,39,40,64],range(10),[0,1,5])]
        return ["family","handle","length","maximum","descriptor-mode","buffer-mode","returned-mode"], output
    if family == "native-symbolic-link-query-alignment":
        return ["family","handle","length","maximum","descriptor-mode","buffer-mode","returned-mode"], [(4,h,0x1234,m,d,0,r) for h,m,d,r in itertools.product([0,2,3,4,6],[0,38,39,40,64],[8,9,10],[0,1,2])]
    if family == "native-symbolic-link-query-copy-boundaries":
        return ["family","handle","length","maximum","descriptor-mode","buffer-mode","returned-mode"], [(5,h,0x1234,m,d,0,r) for h,m,d,r in itertools.product([0,2,4,6],[0,38,40,64],range(20,36),[0,1,2])]
    if family == "native-symbolic-link-query-dual-faults":
        descriptors = [0, 6, 7] + list(range(20, 36))
        return ["family","handle","length","maximum","descriptor-mode","buffer-mode","returned-mode"], [(6,h,0x1234,m,d,b,r) for h,m,d,b,r in itertools.product([0,2,4,6],[0,38,40,64],descriptors,[0,6,7],[0,1,2,6,7,8,9])]
    if family == "native-symbolic-link-open-format":
        paths=[r"\KnownDlls\KnownDllPath",r"\KnownDlls32\KnownDllPath",r"\KnownDlls\ntdll.dll",r"\KnownDlls\ntdll.dll\x",r"\KnownDlls\KnownDllPath\x","\\KnownDlls\\",r"\KnownDlls\\x",r"\\KnownDlls",r"\.\KnownDlls",r"\KnownDlls/ntdll.dll",r"\KnownDlls\..",r"\KnownDlls\.","\\",r"\rax-absent-20261012\x",r"\knowndlls"]
        natural=[len(p.encode("utf-16-le")) for p in paths]
        output=[(0,p,natural[p],f,0,0,0,0,a) for p,f,a in itertools.product(range(15),[0,64,1024,1088,4096,4160],[1,0x1000000,0x3000000,0xFFFFFFFF])]
        output += [(1,0,l,f,n,b,0,0,1) for l,n,b,f in itertools.product([0,1,2,3,23,24,25,45,46,47,48,65530,65532,65534,65535],range(4),range(4),[64,1])]
        output += [(2,0,natural[0],f,0,0,s,q,3) for s,q,f in itertools.product(range(3),range(3),[64,1])]
        return ["family","path","length","flags","unicode-offset","buffer-offset","security","qos","access"], output
    raise ValueError("unowned family " + family)


HEX = {"access","flags","source-access","desired","options","source-flags"}

FAMILIES = [
    "native-symbolic-link-open-matrix",
    "native-symbolic-link-open-boundaries",
    "native-symbolic-link-open-flag-boundaries",
    "native-symbolic-link-open-format",
    "native-symbolic-link-open-name-probe",
    "native-symbolic-link-open-reference",
    "native-symbolic-link-open-duplicate-policy",
    "native-symbolic-link-open-duplicate-boundaries",
    "native-symbolic-link-open-duplicate-protected-boundaries",
    "native-symbolic-link-open-duplicate-access",
    "native-symbolic-link-directory-duplicate-access",
    "native-symbolic-link-query-matrix",
    "native-symbolic-link-query-alignment",
    "native-symbolic-link-query-copy-boundaries",
    "native-symbolic-link-query-dual-faults",
]
PROFILES = ["arm64", "x64", "x86", "x86-laa"]


def producer_inventory(producers):
    inventory = [(item["family"], item["profile"]) for item in producers]
    independent = set(itertools.product(FAMILIES, PROFILES)) | {("fresh", "arm64")}
    require(len(inventory) == 61 and set(inventory) == independent,
            "independent producer inventory")


def fresh_capture(text):
    cases = rows(text)
    require([int(item["case"]) for item in cases] == [0, 1, 2] and
            [item["warmup"] for item in cases] == ["1", "0", "0"],
            "fresh case/warmup partition")
    complete = cases[1:]
    require(all(item["done"] == "1" and item["installed"] == "1" and
                item["calls"] == "1" and item["exit"] == "77" and
                item["exact-handle-set"] == "1" and
                item["handles-before"] == item["handles-after"] for item in complete),
            "fresh capture lifetime")
    require(text.splitlines()[-1] == "complete cases=2 warmup=1", "fresh terminal")
    images, entries, returns = (rows(text, prefix) for prefix in ["image ", "entry ", "return "])
    require(all([int(item["case"]) for item in records] == [0, 1, 2]
                for records in [images, entries, returns]),
            "fresh original-entry/true-LR cardinality")
    captures = rows(text, "bytes ")
    sizes = {"output-before": 8, "attributes-before": 48, "unicode-before": 16,
             "name-before": 24, "output-after": 8, "attributes-after": 48}
    for index, (image, entry, returned) in enumerate(zip(images, entries, returns)):
        code = bytes.fromhex(image["bytes"])
        instruction = int.from_bytes(code[:4], "little")
        require(int(image["type"], 16) == 0x1000000 and
                instruction & 0xFFE0001F == 0xD4000001 and
                (instruction >> 5) & 0xFFFF == 312 and code[4:8] == bytes.fromhex("C0035FD6"),
                "fresh live MEM_IMAGE SVC")
        require(entry["function"] == returned["function"] == "312" and
                entry["call"] == returned["call"] == "0" and
                entry["thread"] == returned["thread"] and
                int(entry["pc"], 16) == int(image["entry"], 16) + 4 and
                int(returned["pc"], 16) == int(entry["lr"], 16) + 4 and
                returned["status"] == "00000000",
                "fresh original-entry/true-LR binding")
        records = [item for item in captures if int(item["case"]) == index]
        require(len(records) == 6 and {item["phase"] for item in records} == set(sizes),
                "fresh byte capture inventory")
        by_phase = {item["phase"]: item for item in records}
        data = {phase: bytes.fromhex(item["data"]) for phase, item in by_phase.items()}
        require(all(int(by_phase[phase]["size"]) == size and len(data[phase]) == size
                    for phase, size in sizes.items()), "fresh ABI capture extents")
        attributes, unicode = data["attributes-before"], data["unicode-before"]
        require(attributes == data["attributes-after"] and
                int.from_bytes(attributes[:4], "little") == 48 and
                int.from_bytes(attributes[8:16], "little") != 0 and
                int.from_bytes(attributes[16:24], "little") == int(by_phase["unicode-before"]["address"], 16) and
                int.from_bytes(attributes[24:28], "little") == 0x40 and
                attributes[32:48] == bytes(16) and unicode[:4] == bytes.fromhex("18001A00") and
                int.from_bytes(unicode[8:16], "little") == int(by_phase["name-before"]["address"], 16) and
                data["name-before"] == "KnownDllPath".encode("utf-16-le") and
                data["output-before"] == bytes(8) and int.from_bytes(data["output-after"], "little") != 0,
                "fresh open input/output binding")
        args = [int(value, 16) for value in entry["args"].split(",") if value]
        require(args[:3] == [int(by_phase["output-before"]["address"], 16), 1,
                             int(by_phase["attributes-before"]["address"], 16)] and
                by_phase["output-before"]["address"] == by_phase["output-after"]["address"],
                "fresh argument/address binding")


def main(directory):
    root = Path(directory)
    identity = json.loads((root / "native-symbolic-link-open-producer-identities.json").read_text())
    expected = json.loads((root / "native-symbolic-link-open-producer-inputs.json").read_text())
    require(len(identity["producers"]) == len(expected), "producer identity count")
    producer_inventory(identity["producers"])
    producer_inventory(expected)
    require(identity["windows-version"] == "10.0.29683", "actual Windows build")
    require(identity["host-machine"] == "ARM64", "actual native host machine")
    libraries = identity["native-libraries"]
    expected_libraries = [
        (r"C:\Windows\System32\ntdll.dll", 4609312, 0xAA64,
         "502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f"),
        (r"C:\Windows\SysWOW64\ntdll.dll", 1849976, 0x14C,
         "ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e"),
    ]
    require([(item["path"], item["bytes"], item["machine"], item["sha256"])
             for item in libraries] == expected_libraries,
            "captured installed native DLL identities")
    total=0
    observed={}
    for producer, specification in zip(identity["producers"],expected):
        require(all(producer[k] == v for k,v in specification.items()), "producer partition binding")
        source=root / producer["source"]
        require(digest(source) == producer["source-sha256"] == producer["source_identity"]["sha256"], "actual compiled source binding")
        require(source.stat().st_size == producer["source_identity"]["bytes"], "actual source size")
        exe=producer["executable_identity"]
        require(exe["machine"] == {"arm64":0xAA64,"x64":0x8664,"x86":0x14C,"x86-laa":0x14C}[producer["profile"]], "actual PE machine")
        if producer["profile"].startswith("x86"):
            require(bool(exe["characteristics"] & 0x20) == (producer["profile"] == "x86-laa"), "actual PE LAA")
        log=root / producer["log"]
        require(digest(log) == producer["log_identity"]["sha256"] and log.stat().st_size == producer["log_identity"]["bytes"], "actual native stdout binding")
        text=log.read_text()
        require("failure=" not in text,"failed native producer")
        if producer["family"] == "fresh":
            fresh_capture(text)
            continue
        width=4 if producer["profile"].startswith("x86") else 8
        fields,inputs=partition(producer["family"],width)
        records=rows(text)
        require(len(records)==len(inputs) and text.splitlines()[-1]==f"complete cases={len(inputs)}", "independent input partition cardinality: " + producer["family"])
        semantic=[]
        for index,(record,values) in enumerate(zip(records,inputs)):
            require(int(record["case"])==index and int(record["width"])==width,"case ordinal/width")
            require(tuple(int(record[key],16 if key in HEX else 10) for key in fields)==tuple(values),"independent input partition differs: " + producer["family"]+" case="+str(index))
            require(record["handles-before"]==record["handles-after"],"native owned lifetime")
            require(record.get("exception","00000000")=="00000000","native caller exception")
            # Status, grant, flags, lifetime deltas and protection changes are
            # independent of ASLR and the process's pre-existing handle count.
            semantic.append(tuple((key,value) for key,value in sorted(record.items()) if key in ["status","exception","ok","error","grant","source-live","handle-flags","wait"] or key.endswith("protect") or key.endswith("protection") or key.endswith("next")))
        observed[producer["family"],producer["profile"]]=semantic
        total+=len(records)
    families={key[0] for key in observed}
    require(total == 131944 and families == set(FAMILIES),
            "independent complete corpus cardinality")
    for family in families:
        require(observed[family,"arm64"] == observed[family,"x64"], "native64 semantic mismatch " + family)
        require(observed[family,"x86"] == observed[family,"x86-laa"], "WoW64 semantic mismatch " + family)
    manifest = json.loads((root / "evidence-manifest.json").read_text())
    files = {str(path.relative_to(root)) for path in root.rglob("*")
             if path.is_file() and "__pycache__" not in path.parts and
             path.name != "evidence-manifest.json"}
    require(set(manifest) == files, "complete evidence file inventory")
    for filename, binding in manifest.items():
        path = root / filename
        require(path.stat().st_size == binding["bytes"] and digest(path) == binding["sha256"],
                "evidence file binding: " + filename)
    print(json.dumps({"actual-producers":len(expected),"independent-input-rows":total,"families":len(families),"same-width-status-grant-guard-agreement":True,"fresh-complete-cases":2},sort_keys=True))


if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--corpus",default=str(Path(__file__).parent))
    arguments=parser.parse_args()
    main(arguments.corpus)
