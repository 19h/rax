from pathlib import Path
import datetime,hashlib,json,shutil,subprocess
B=Path("C:/Windows/Temp/assist-native-rax-20261010");S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154");F=S/"eri-obsolete-symbols-cleanup-v1.json";assert not F.exists()
def identity(p):
 h=hashlib.sha256()
 with p.open("rb") as f:
  for b in iter(lambda:f.read(1<<20),b""):h.update(b)
 return {"path":str(p),"bytes":p.stat().st_size,"sha256":h.hexdigest()}
def save():F.write_text(json.dumps(r,indent=2)+"\n")
p=subprocess.run(["powershell.exe","-NoProfile","-NonInteractive","-Command","Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(cargo|rustc|link|cl)(.exe)?$' -and $_.CommandLine -like '*src-alpc-candidate*' } | Select-Object ProcessId,Name,CommandLine | ConvertTo-Json -Depth 3"],capture_output=True,timeout=30);assert p.returncode==0 and not p.stdout.strip(),p.stdout
r={"state":"preparing","utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"free-before":shutil.disk_usage(B).free,"retired-process-inspection":p.stdout.decode(errors="replace"),"deleted":[],"scope":"Only exact retired ALPC standalone PDB files and a verified incomplete ERI carrier retention copy; preserve all source, live archives, executables, retained complete files and current ERI symbols."};save()
kept=[]
for c in [1,2]:
 a=json.loads((S/("alpc-retained-owning-candidate"+str(c)+".json")).read_text());assert a["state"]=="finished"
 for x in a["files"]:
  actual=identity(Path(x["retained"]));assert (actual["bytes"],actual["sha256"])==(x["bytes"],x["sha256"]);kept.append(actual)
archives=json.loads((S/"native-eri-candidate1-final-owning-archive-identities.json").read_text())["archives"]
for x in archives:
 actual=identity(Path(x["path"]));assert (actual["bytes"],actual["sha256"])==(x["bytes"],x["sha256"]);kept.append(actual)
target=B/"src-eri-candidate1/target/retained-owning-candidate1/carrier.lib";expected=next(x for x in archives if x["path"].endswith(".lib"))
if target.exists():
 row=identity(target);assert row["bytes"]<expected["bytes"] and row["sha256"]!=expected["sha256"];row["reason"]="incomplete byte copy from failed retention";r["deleted"].append(row);save();target.unlink()
for c in [1,2]:
 root=B/("src-alpc-candidate"+str(c)+"/target/normal-candidate"+str(c)+"/debug")
 for folder in [root,root/"deps"]:
  if not folder.exists():continue
  for f in sorted(folder.iterdir()):
   if f.is_file() and f.suffix.lower()==".pdb":
    row=identity(f);row["reason"]="retired standalone debugger symbols, not a source/executed image or archive";r["deleted"].append(row);save();f.unlink()
for x in kept:assert identity(Path(x["path"]))==x,x["path"]
r.update(state="finished",**{"free-after":shutil.disk_usage(B).free,"preserved-byte-identities":kept});save();print(json.dumps({"state":r["state"],"deleted-files":len(r["deleted"]),"deleted-logical-bytes":sum(x["bytes"] for x in r["deleted"]),"free-before":r["free-before"],"free-after":r["free-after"]}))
