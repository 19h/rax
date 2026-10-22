from pathlib import Path
import json,shutil,datetime,ctypes
B=Path("C:/Windows/Temp/assist-native-rax-20261010");S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154");T=B/"src-eri-candidate1/target/retained-owning-candidate1"
r={"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"free-bytes":shutil.disk_usage(B).free,"targets":[]}
api=ctypes.WinDLL("kernel32",use_last_error=True).GetCompressedFileSizeW;api.argtypes=[ctypes.c_wchar_p,ctypes.POINTER(ctypes.c_uint32)];api.restype=ctypes.c_uint32
if T.exists():
 for p in T.iterdir():
  if p.is_file():
   st=p.stat();high=ctypes.c_uint32();low=api(str(p),ctypes.byref(high));r["targets"].append({"path":str(p),"bytes":st.st_size,"attributes":st.st_file_attributes,"reported-physical-bytes":(high.value<<32)|low})
r["current-source-archives"]=json.loads((S/"native-eri-candidate1-final-owning-archive-identities.json").read_text())["archives"]
f=S/"eri-retention-inspection-v2.json";assert not f.exists();f.write_text(json.dumps(r,indent=2)+"\n");print(json.dumps(r))
