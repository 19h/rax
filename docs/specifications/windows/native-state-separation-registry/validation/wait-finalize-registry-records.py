from pathlib import Path
import datetime,json,subprocess,sys,time,traceback
Q=Path(__file__).parent
S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
RECORD=Q/'registry-record-finalization.json';assert not RECORD.exists()
x={'state':'waiting-for-final-native-build','utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
def save():RECORD.write_text(json.dumps(x,indent=2)+'\n')
save()
try:
 deadline=time.monotonic()+1800
 while True:
  native=json.loads((S/'registry-native-epoch1-gates.json').read_text())
  assert native['state']!='failed',native.get('error','native final gate failed')
  if native['state']=='finished':
   assert native['exit-code']==0
   break
  assert time.monotonic()<deadline
  time.sleep(3)
 with (Q/'registry-record-finalization.log').open('wb') as out:
  child=subprocess.Popen([sys.executable,'-B',str(Q/'finalize-registry-records.py')],stdout=out,stderr=subprocess.STDOUT)
  x.update(state='finalizing',**{'child-pid':child.pid});save();code=child.wait()
 assert code==0,code
 x.update(state='finished',**{'exit-code':0})
except BaseException:x.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()})
finally:x['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(x),flush=True);raise SystemExit(x['exit-code'])
