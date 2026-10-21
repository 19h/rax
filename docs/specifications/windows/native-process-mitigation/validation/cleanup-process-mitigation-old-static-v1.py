from pathlib import Path
import json,shutil,datetime
B=Path("C:/Windows/Temp/assist-native-rax-20261010")
S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
roots=[B/"src-file-open-epoch2/target/normal",*[B/("src-"+n+"-candidate1/target/normal-candidate1") for n in ["file-section","current-el","volume-query","nls-sorting"]]]
r={"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"free-before":shutil.disk_usage(B).free,"deleted":[],"scope":"Only obsolete standalone primary Cargo static libraries and intermediates from five completed targets. All .exe/.rlib files, native dependency build outputs, source/log records, actual owning carrier/core/callers and separately retained artifacts remain intact."}
for root in roots:
 assert root.is_dir()
 for directory in [root/"debug",root/"debug/deps"]:
  for p in directory.iterdir():
   if not p.is_file():continue
   if not (p.name.startswith(("rax","librax")) and p.suffix in [".lib",".rmeta",".o",".obj"]):continue
   assert p.suffix not in [".rlib",".exe"]
   r["deleted"].append({"path":str(p),"bytes":p.stat().st_size});p.unlink()
r["free-after"]=shutil.disk_usage(B).free
(S/"process-mitigation-old-static-cleanup-v1.json").write_text(json.dumps(r,indent=2)+"\n")
print("deleted",len(r["deleted"]),"files; recovered",r["free-after"]-r["free-before"],"bytes; free",r["free-after"],flush=True)
