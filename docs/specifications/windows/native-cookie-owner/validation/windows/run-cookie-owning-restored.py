from pathlib import Path
import datetime,hashlib,json,os,subprocess,sys,tarfile,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R=S/'cookie-owner-final-owning-restored.json'
L=B/'cookie-owner-final-owning-restored.lock'
def need(x,m):
 if not x:raise ValueError(m)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 need(__debug__,'optimized driver')
 need(not R.exists(),'prior restore already requested')
 need(json.loads((S/'cookie-owner-final-owning-recovery.json').read_text())['exit-code']==1,'retain preceding failure')
 fd=os.open(L,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
 row={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'restore checksum-bound missing files in task-owned dependency caches; owning checks only'}
 def save():R.write_text(json.dumps(row,indent=2)+'\n')
 save()
 try:
  cfg={str(p):sha(p) for p in [B/'src/.cargo/config.toml',B/'assist/rust/assist-rs/.cargo/config.toml']}
  rows=json.loads((S/'cookie-dependency-restore-inputs.json').read_text())['files'];restored=0
  with tarfile.open(S/'cookie-dependency-restore.tar.gz') as tar:
   need(len(tar.getmembers())==len(rows),'complete restore archive')
   for item in rows:
    name=item['path'];parts=Path(name).parts
    need(not Path(name).is_absolute() and '..' not in parts and parts[0] in ['dependencies','assist-dependencies'],'bounded path')
    data=tar.extractfile(name).read()
    need(len(data)==item['bytes'] and hashlib.sha256(data).hexdigest()==item['sha256'],'expected vendored checksum')
    dst=B/name
    if dst.exists():need(dst.is_file() and sha(dst)==item['sha256'],'existing cache mismatch')
    else:
     dst.parent.mkdir(parents=True,exist_ok=True)
     with dst.open('xb') as f:f.write(data)
     restored+=1
  for item in rows:need(sha(B/item['path'])==item['sha256'],'restored file identity')
  need(cfg=={p:sha(Path(p)) for p in cfg},'configuration retained byte-for-byte')
  need((B/'assist/vendor/rax').resolve()==(B/'src').resolve(),'owned junction still selects source')
  row.update({'restored-files':restored,'verified-files':len(rows),'configs':cfg,'archive-sha256':sha(S/'cookie-dependency-restore.tar.gz')})
  script=S/'assist-native-cookie-owner-final-restored-owned.ps1'
  argv=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(script)]
  with (S/'cookie-owner-final-restored-driver.log').open('wb') as log:
   child=subprocess.Popen(argv,stdout=log,stderr=subprocess.STDOUT)
   row.update({'state':'running','child-pid':child.pid,'argv':argv});save()
   row['exit-code']=child.wait()
  need(row['exit-code']==0,'restored owning checks failed')
  archive=json.loads((S/'native-cookie-owner-final-restored-owning-archive-hashes.json').read_text())
  need(len(archive['sources'])==310 and archive['core-features']==[],'source and features binding')
  need(len(archive['archives'])==2 and all(x['structural-member-walk']=='pass' for x in archive['archives']),'complete archives')
  row['state']='finished'
 except BaseException:
  row.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
 finally:
  row['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();L.unlink()
 print(json.dumps(row,indent=2),flush=True)
 return row['exit-code']
if __name__=='__main__':sys.exit(main())
