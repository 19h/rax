from pathlib import Path
import datetime,hashlib,json,os,subprocess,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'src-continue-epoch1';F=S/'continue-native-owning-link-b.json';assert not F.exists();state={'state':'waiting-for-materialized-source','pid':os.getpid(),'steps':[]}
def save():F.write_text(json.dumps(state,indent=2)+'\n')
save()
try:
 deadline=time.monotonic()+7200
 while True:
  try:d=json.loads((S/'continue-native-epoch1-gates.json').read_text())
  except json.JSONDecodeError:time.sleep(0.25);continue
  assert d['state']!='failed'
  if d.get('verified-source-inputs')==25069:break
  assert time.monotonic()<deadline;time.sleep(2)
 own=json.loads((S/'continue-native-owning-epoch1.json').read_text());assert own['state']=='waiting-for-focus',own['state']
 manifest=json.loads((S/'continue-owning-source-hashes-epoch1.json').read_text())
 for row in manifest['sources']:
  raw=(R/row['path']).read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']),row['path']
 link=B/'assist/vendor/rax';assert link.is_junction() and link.resolve()==(B/'src-alert-epoch4').resolve();before=os.readlink(link);os.rmdir(link)
 try:subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(link),str(R)],check=True,capture_output=True)
 except BaseException:subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(link),before],check=True);raise
 assert link.resolve()==R.resolve();state.update({'state':'finished','exit-code':0,'before':before,'after':str(link.resolve()),'source-inputs':len(manifest['sources']),'qualification':'Explicit verified handoff prepares the new target for the already waiting owning controller, whose generated admission expects that target to be selected. Prior owning archive/ordinary runs are finished; no build is restarted.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
