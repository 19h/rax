from pathlib import Path
import json,shutil,datetime
B=Path("C:/Windows/Temp/assist-native-rax-20261010")
S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
roots=[B/"src-volume-query-candidate1/target/normal-candidate1",B/"src-nls-sorting-candidate1/target/normal-candidate1"]
r={"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"free-before":shutil.disk_usage(B).free,"deleted":[],"scope":"Only completed older standalone Cargo .pdb files. Native unit images, actual owning archives/callers, four retained copies and all source/log evidence preserved."}
for root in roots:
 assert root.is_dir()
 for p in root.rglob("*.pdb"):
  assert p.is_relative_to(root) and not p.is_symlink()
  r["deleted"].append({"path":str(p),"bytes":p.stat().st_size});p.unlink()
r["free-after"]=shutil.disk_usage(B).free
(S/"process-mitigation-old-symbol-cleanup-v1.json").write_text(json.dumps(r,indent=2)+"\n")
print("recovered",r["free-after"]-r["free-before"],"bytes; free",r["free-after"],flush=True)
