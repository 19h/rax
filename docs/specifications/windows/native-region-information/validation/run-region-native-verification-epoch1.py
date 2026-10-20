from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,sys,tarfile,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');SOURCE=B/'src-region-epoch1';VERIFY=B/'verify-region-epoch1';PREFIX='docs/specifications/windows/native-region-information/';ROUND=int(sys.argv[1]);assert ROUND in [1,2];RECORD=S/('region-epoch1-final-check-windows-round'+str(ROUND)+'.json');assert not RECORD.exists()
state={'state':'preparing','pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'round':ROUND,'steps':[]}
def save():RECORD.write_text(json.dumps(state,indent=2)+'\n')
def identity(path):
 h=hashlib.sha256();size=0
 with path.open('rb')as f:
  for chunk in iter(lambda:f.read(1048576),b''):h.update(chunk);size+=len(chunk)
 return {'bytes':size,'sha256':h.hexdigest()}
save()
try:
 manifest=json.loads((S/'region-source-epoch1.json').read_text());meta=json.loads((S/('region-epoch1-verification-docs-round'+str(ROUND)+'.json')).read_text())
 if ROUND==1:
  assert not VERIFY.exists();VERIFY.mkdir()
  for row in manifest['files']:
   if row['path'].startswith(PREFIX):continue
   target=VERIFY/row['path'];target.parent.mkdir(parents=True,exist_ok=True)
   try:os.link(SOURCE/row['path'],target)
   except OSError:shutil.copy2(SOURCE/row['path'],target)
 else:
  prior=json.loads((S/'region-epoch1-final-check-windows-round1.json').read_text());assert prior['state']=='finished'and prior['exit-code']==0
  old=json.loads((S/'region-epoch1-verification-docs-round1.json').read_text())
  for row in old['files']:assert identity(VERIFY/row['path'])=={k:row[k]for k in ['bytes','sha256']},row['path']
  doc=VERIFY/PREFIX;assert doc.resolve().is_relative_to(VERIFY.resolve())and not doc.is_symlink();shutil.rmtree(doc)
 archive=S/meta['archive'];assert identity(archive)=={k:meta[k]for k in ['bytes','sha256']}
 with tarfile.open(archive,'r:gz')as tar:
  members=tar.getmembers();assert len(members)==len(meta['files'])and{m.name for m in members}=={x['path']for x in meta['files']}
  assert all(m.isfile()and m.name.startswith(PREFIX)and'..'not in Path(m.name).parts for m in members)
  tar.extractall(VERIFY,filter='data')
 for row in meta['files']:assert identity(VERIFY/row['path'])=={k:row[k]for k in ['bytes','sha256']},row['path']
 command=[str(B/'python/python.exe'),'-B',str(VERIFY/PREFIX/'check_validation.py'),'--host','windows','--source',str(VERIFY)];log=S/('region-epoch1-final-check-windows-round'+str(ROUND)+'.log')
 with log.open('wb')as out:
  child=subprocess.Popen(command,cwd=VERIFY,stdout=out,stderr=subprocess.STDOUT);state.update({'state':'checking','child-pid':child.pid});save();code=child.wait()
 state['steps'].append({'name':'portable-proof','argv':command,'exit-code':code,'log':log.name,**identity(log)});assert code==0,log.read_text(errors='replace')
 state.update({'state':'finished','exit-code':0,'verification-root':str(VERIFY),'source-root':str(SOURCE),'source-inputs':len(manifest['files']),'document-inputs':len(meta['files']),'envelope':meta['envelope'],'qualification':'Separate verification source; native product source remains unchanged. Only this task-owned verification documentation is replaced between rounds.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
