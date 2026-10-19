from pathlib import Path
import datetime,json,os,subprocess,sys,time,traceback
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=S/'wait-owner-epoch4-queue.json'
assert not R.exists(),'observer queue already recorded'
a={'pid':os.getpid(),'state':'waiting','utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
def save():R.write_text(json.dumps(a,indent=2)+'\n')
save()
try:
 deadline=time.monotonic()+1800
 while True:
  record=json.loads((S/'wait-native-owning-epoch4.json').read_text())
  if record['state']=='finished':assert record['exit-code']==0;break
  assert record['state']!='failed',record.get('error','owner failed')
  assert time.monotonic()<deadline,'owning validation timed out';time.sleep(3)
 a['state']='observing';save()
 child=subprocess.Popen([sys.executable,'-B',str(S/'run-wait-owner-epoch4.py')]);a['child-pid']=child.pid;save();a['exit-code']=child.wait();a['state']='finished' if a['exit-code']==0 else 'failed'
except BaseException:a.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:a['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(a),flush=True);raise SystemExit(a['exit-code'])
