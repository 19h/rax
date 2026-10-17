from pathlib import Path
import json,subprocess,sys,time
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
deadline=time.monotonic()+7200
while True:
 if time.monotonic()>deadline:raise TimeoutError('System WNF validation did not complete')
 try:
  r=json.loads((S/'wnf-system-final-launch.json').read_text())
  if r.get('state')=='failed':raise RuntimeError(r)
  ready=r.get('state')=='finished'and r.get('exit-code')==0
  for host in ['macos','linux']:
   for stage,count in [('wnf-system-final',4),('system-owning',6 if host=='macos'else 4)]:
    rows=json.loads((B/(host+'-'+stage+'-gates.json')).read_text());ready&=len(rows)==count and all(x['exit-code']==0 for x in rows)
  if ready:break
 except (FileNotFoundError,json.JSONDecodeError):pass
 time.sleep(10)
p=S/'copy-system-completed-artifacts.py';p.write_text("""from pathlib import Path
import json,shutil
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');b=Path('C:/Windows/Temp/assist-native-rax-20261010')
r=json.loads((s/'wnf-system-final-launch.json').read_text());assert r['state']=='finished'and r['exit-code']==0
for n in ['native-wnf-system-final-build-user-artifacts.jsonl','native-wnf-system-final-after-artifacts.jsonl','native-wnf-system-final-after-artifacts-stderr.log']:shutil.copyfile(b/n,s/n)
""")
a=['prlctl','exec','a5b14f9a-d292-4610-a04d-df4ca4337c9d',r'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe','-B',r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\copy-system-completed-artifacts.py']
r=subprocess.run(a,capture_output=True,text=True,timeout=120);(B/'completed-artifact-copy.json').write_text(json.dumps({'argv':a,'exit-code':r.returncode,'stdout':r.stdout,'stderr':r.stderr},indent=2)+'\n')
if r.returncode:raise RuntimeError('native artifact copy failed')
for p in [B/'collect-system-validation.py',B/'rax-source/docs/specifications/windows/native-wnf-system-subscribe/validation-check.py',B/'stress-system-validation.py']:
 subprocess.run([sys.executable,'-B',str(p)],check=True)
print('Completed System WNF validation collection and adversarial checks',flush=True)
