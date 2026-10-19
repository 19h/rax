from pathlib import Path
import datetime,json,subprocess,sys,time,traceback
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
RECORD=S/'redirection-owner-epoch1-dispatch.json'
assert not RECORD.exists()
state={'state':'waiting-for-validated-owner','utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
def save():RECORD.write_text(json.dumps(state,indent=2)+'\n')
save()
try:
 deadline=time.monotonic()+1800
 while True:
  owner=json.loads((S/'registry-native-owning-epoch1.json').read_text())
  assert owner['state']!='failed',owner.get('error','native owning validation failed')
  if owner['state']=='finished':
   assert owner['exit-code']==0
   break
  assert time.monotonic()<deadline,'native owner wait expired'
  time.sleep(5)
 with (S/'redirection-owner-epoch1-controller.log').open('wb') as log:
  child=subprocess.Popen([sys.executable,'-B',str(S/'run-redirection-owner-epoch1.py')],stdout=log,stderr=subprocess.STDOUT)
  state.update(state='running-observer',**{'child-pid':child.pid});save();code=child.wait()
 assert code==0,code
 state.update(state='finished',**{'exit-code':0})
except BaseException:state.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
