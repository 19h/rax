from pathlib import Path
import ctypes,datetime,hashlib,json,shutil,subprocess
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
plan=json.loads((S/'alpc-retained-compression-plan-v1.json').read_text());allowed={str(B/x) for x in ['src-process-mitigation-candidate2/target/retained-owning-candidate2','src-nls-sorting-candidate1/target/retained-owning-candidate1','src-volume-query-candidate1/target/retained-owning-candidate1']}
record=S/'alpc-retained-compression-v1.json';assert not record.exists()
api=ctypes.WinDLL('kernel32',use_last_error=True).GetCompressedFileSizeW;api.argtypes=[ctypes.c_wchar_p,ctypes.POINTER(ctypes.c_uint32)];api.restype=ctypes.c_uint32
j={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running','pid':__import__('os').getpid(),'free-before':shutil.disk_usage(B).free,'files':[],'scope':'NTFS lossless compression of12 explicit old task-owned retained archive/executable byte copies. Keep paths, logical bytes and SHA256 unchanged. No source, active output, Defender setting or user build altered.'}
def save():record.write_text(json.dumps(j,indent=2)+'\n')
def identity(p):
 d=hashlib.sha256()
 with p.open('rb') as f:
  for block in iter(lambda:f.read(1<<20),b''):d.update(block)
 return {'bytes':p.stat().st_size,'sha256':d.hexdigest()}
def physical(p):
 high=ctypes.c_uint32();ctypes.set_last_error(0);low=api(str(p),ctypes.byref(high));assert low!=0xFFFFFFFF or ctypes.get_last_error()==0;return (high.value<<32)|low
save()
for index,row in enumerate(plan['files']):
 p=Path(row['path']);assert str(p.parent) in allowed and p.suffix.lower() in ['.exe','.lib','.rlib'] and p.stat().st_size==row['bytes']
 entry={'path':str(p),'before':identity(p),'physical-before':physical(p)};j['current-file']=str(p);save()
 result=subprocess.run(['compact.exe','/C','/I','/Q',str(p)],capture_output=True,timeout=300)
 log=S/f'alpc-retained-compression-v1-{index}.log';log.write_bytes(result.stdout+result.stderr);entry.update({'compact-exit-code':result.returncode,'log':log.name,'after':identity(p),'physical-after':physical(p)});assert entry['before']==entry['after'];assert result.returncode==0,result.stdout;j['files'].append(entry);save()
j.update(state='finished',**{'free-after':shutil.disk_usage(B).free,'physical-before':sum(x['physical-before'] for x in j['files']),'physical-after':sum(x['physical-after'] for x in j['files'])});save();print(json.dumps({'state':j['state'],'files':len(j['files']),'free-before':j['free-before'],'free-after':j['free-after'],'physical-before':j['physical-before'],'physical-after':j['physical-after']}))
