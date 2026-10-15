from pathlib import Path
import hashlib,importlib.util,json,sys
sys.dont_write_bytecode=True

def main():
    if not __debug__: raise ValueError("optimized checks rejected")
    root=Path(__file__).parent
    for row in json.loads((root/"identity.json").read_text())["files"]:
        data=(root/row["path"]).read_bytes()
        if len(data)!=row["bytes"] or hashlib.sha256(data).hexdigest()!=row["sha256"]:raise ValueError(row["path"])
    spec=importlib.util.spec_from_file_location("native_capture_inspector",root/"inspect-lpc-missing-controls.py")
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    record=json.loads((root/"native-lpc-missing-controls-identities.json").read_text())
    if len(record["producers"])!=4 or record["observations"]!=352:raise ValueError("native partition")
    for profile,width in [("arm64",8),("x64",8),("x86",4),("x86-laa",4)]:
        module.validate((root/("native-lpc-missing-controls-"+profile+".log")).read_text(),width)
    status=json.loads((root/"native-secure-connect-status.json").read_text())
    if status["status"]!="C0000078" or status["win32-error"]!=1337:raise ValueError("native status mapping")
    spec=importlib.util.spec_from_file_location("readonly_inspector",root/"inspect-lpc-readonly-capture.py")
    readonly=importlib.util.module_from_spec(spec);spec.loader.exec_module(readonly)
    ro=json.loads((root/"native-lpc-readonly-capture-identities.json").read_text())
    if len(ro["producers"])!=4 or ro["observations"]!=44:raise ValueError("readonly native partition")
    for profile,width in [("arm64",8),("x64",8),("x86",4),("x86-laa",4)]:
        rows=readonly.validate((root/("native-lpc-readonly-capture-"+profile+".log")).read_text(),width)
        for row in rows:
            expected="C0000005" if row["name"] in {"output","client-view","server-view","message","information"} else "C0000034"
            if row["status"]!=expected or row["exception"]!="00000000":raise ValueError("readonly result")
            prefix="00000000" if width==4 and row["name"]!="output" else "a5a5a5a5"
            if row["output-prefix"]!=prefix:raise ValueError("readonly copyback")
    print("Verified 8 actual producers, 396 exact native observations, 0 caller exceptions, 12 closes and invalid-SID translation")

if __name__=="__main__":
    main()
