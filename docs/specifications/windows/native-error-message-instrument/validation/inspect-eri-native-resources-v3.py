from pathlib import Path
import datetime,json,shutil,subprocess
B=Path("C:/Windows/Temp/assist-native-rax-20261010");S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
r={"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"free-bytes":shutil.disk_usage(B).free,"retired-libraries":[],"failed-pdb":None}
for c in [1,2]:
 root=B/("src-alpc-candidate"+str(c)+"/target/normal-candidate"+str(c)+"/debug")
 for folder in [root,root/"deps"]:
  if folder.exists():
   for p in folder.iterdir():
    if p.is_file() and p.suffix in [".rlib",".lib"] and p.name.startswith(("librax","rax")) and p.stat().st_size>1_000_000:r["retired-libraries"].append({"path":str(p),"bytes":p.stat().st_size})
p=B/"src-eri-candidate1/target/normal-candidate2/debug/deps/rax.pdb"
if p.exists():r["failed-pdb"]={"path":str(p),"bytes":p.stat().st_size,"attributes":p.stat().st_file_attributes}
f=S/"eri-native-resources-v3.json";assert not f.exists();f.write_text(json.dumps(r,indent=2)+"\n");print(json.dumps(r))
