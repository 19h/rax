from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,sys,tarfile,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');SOURCE=B/'src-session-alias-epoch1';R=B/'verify-session-alias-epoch1';PREFIX='docs/specifications/windows/native-session-directory-alias/';number=int(sys.argv[1]);assert number in [1,2];RECORD=S/('session-alias-proof-windows-round'+str(number)+'.json');assert not RECORD.exists();state={'state':'preparing','pid':os.getpid(),'round':number,'steps':[]}
def save():RECORD.write_text(json.dumps(state,indent=2)+'\n')
def identity(path):
 h=hashlib.sha256();size=0
 with path.open('rb')as f:
  for chunk in iter(lambda:f.read(1<<20),b''):h.update(chunk);size+=len(chunk)
 return {'bytes':size,'sha256':h.hexdigest()}
save()
try:
 meta=json.loads((S/('session-alias-proof-round'+str(number)+'.json')).read_text());changed={row['path']for row in meta['files']};frozen=json.loads((S/'session-alias-source-epoch1.json').read_text())
 if number==1:
  assert not R.exists();R.mkdir()
  for row in frozen['files']:
   if row['path']in changed:continue
   target=R/row['path'];target.parent.mkdir(parents=True,exist_ok=True)
   try:os.link(SOURCE/row['path'],target)
   except OSError:shutil.copy2(SOURCE/row['path'],target)
 else:
  prior=json.loads((S/'session-alias-proof-windows-round1.json').read_text());assert prior['state']=='finished'and prior['exit-code']==0
  old=json.loads((S/'session-alias-proof-round1.json').read_text())
  for row in old['files']:assert identity(R/row['path'])=={k:row[k]for k in ['bytes','sha256']},row['path']
  docs=R/PREFIX;assert docs.resolve().is_relative_to(R.resolve())and not docs.is_symlink();shutil.rmtree(docs)
 archive=S/meta['archive'];assert identity(archive)=={k:meta[k]for k in ['bytes','sha256']}
 with tarfile.open(archive,'r:gz')as tar:
  members=tar.getmembers();assert len(members)==len(changed)and{row.name for row in members}==changed
  assert all(row.isfile()and not Path(row.name).is_absolute()and'..'not in Path(row.name).parts for row in members)
  tar.extractall(R,filter='data')
 for row in meta['files']:assert identity(R/row['path'])=={k:row[k]for k in ['bytes','sha256']},row['path']
 argv=[str(B/'python/python.exe'),'-B',str(R/PREFIX/'check_validation.py'),'--host','windows','--source',str(R)];log=S/('session-alias-proof-windows-round'+str(number)+'.log')
 with log.open('wb')as out:
  child=subprocess.Popen(argv,cwd=R,stdout=out,stderr=subprocess.STDOUT);state.update({'state':'running','child-pid':child.pid});save();code=child.wait()
 state['steps'].append({'name':'portable-proof','argv':argv,'exit-code':code,'log':log.name,**identity(log)});assert code==0,log.read_text(errors='replace')
 state.update({'state':'finished','exit-code':0,'envelope':meta['envelope'],'scope':'Separateverificationtree; historicalfrozeninputs and explicitpublicprojections both checked. Productsource and retainedexecutedartifacts remainunchanged.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
