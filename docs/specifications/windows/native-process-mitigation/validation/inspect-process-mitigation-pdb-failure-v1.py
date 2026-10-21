from pathlib import Path
import json,shutil,datetime
B=Path("C:/Windows/Temp/assist-native-rax-20261010")
S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
r={"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"free-bytes":shutil.disk_usage(B).free,"files":[]}
for n in ["src-process-mitigation-candidate2/target/normal-candidate2/debug/deps/rax-b799307a7949aa6b.pdb","src-process-mitigation-candidate2/target/normal-candidate2/debug/deps/rax-b799307a7949aa6b.exe","src-process-mitigation-candidate1/target/normal-candidate1"]:
 p=B/n
 if p.is_file():r["files"].append({"path":str(p),"bytes":p.stat().st_size})
 elif p.is_dir():r["files"].append({"path":str(p),"bytes":sum(x.stat().st_size for x in p.rglob("*") if x.is_file())})
(S/"process-mitigation-pdb-failure-disk-v1.json").write_text(json.dumps(r,indent=2)+"\n")
print(json.dumps(r),flush=True)
