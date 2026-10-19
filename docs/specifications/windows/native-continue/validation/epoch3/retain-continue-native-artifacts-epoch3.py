from pathlib import Path
import datetime,hashlib,json,os,shutil,traceback,time
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');RECORD=S/'continue-native-retained-artifacts-epoch3.json';R=B/'src-continue-epoch3/target/retained-owning-epoch3';assert not RECORD.exists()and not R.exists();state={'state':'copying','pid':os.getpid(),'artifacts':[]}
def save():RECORD.write_text(json.dumps(state,indent=2)+'\n')
def identity(path):
 h=hashlib.sha256();size=0
 with path.open('rb')as stream:
  for raw in iter(lambda:stream.read(1048576),b''):h.update(raw);size+=len(raw)
 return{'path':str(path),'bytes':size,'sha256':h.hexdigest()}
save()
try:
 while True:
  try:capi=json.loads((S/'continue-owning-capi-epoch3.json').read_text())
  except(FileNotFoundError,json.JSONDecodeError):capi={}
  if capi.get('state')in['finished','failed']:break
  time.sleep(5)
 assert capi['state']=='finished'and capi['exit-code']==0
 ordinary=json.loads((S/'continue-native-ordinary-epoch3.json').read_text());assert ordinary['state']=='finished'and ordinary['exit-code']==0
 archives=json.loads((S/'native-continue-epoch3-final-owning-archive-identities.json').read_text());rows=[*archives['archives'],ordinary['executable-after'],capi['executable-after']];R.mkdir(parents=True)
 for row in rows:
  source=Path(row['path']);before=identity(source);assert(before['bytes'],before['sha256'])==(row['bytes'],row['sha256']);target=R/source.name;shutil.copy2(source,target);copied=identity(target);after=identity(source);assert before==after and(copied['bytes'],copied['sha256'])==(row['bytes'],row['sha256']);state['artifacts'].append({'source':before,'retained':copied});save()
 state.update({'state':'finished','exit-code':0,'scope':'Private byte copies preserve the actual owning archives and observer executable before later semantic-group rebuilds reuse their Cargo output paths. Runtime DLLs are not copied.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
