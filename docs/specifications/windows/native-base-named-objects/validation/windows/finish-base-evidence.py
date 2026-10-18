from pathlib import Path
import datetime,json,subprocess,sys
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
for name in ['base-final-launch.json','base-final-owned-launch.json']:
 row=json.loads((S/name).read_text(encoding='utf-8-sig'))
 if row['state']!='finished' or row['exit-code']!=0:raise RuntimeError('native driver not complete '+name)
rows=[]
for script,log in [('verify-base-final.py','native-base-final-source-post-run.log'),('verify-base-post-run.py','native-base-final-post-run-verifier.log')]:
 p=subprocess.run([sys.executable,'-B',str(S/script)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (S/log).write_bytes(p.stdout);rows.append({'script':script,'log':log,'exit-code':p.returncode})
 if p.returncode:raise RuntimeError(script+' failed')
(S/'base-final-post-run-completion.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'checks':rows},indent=2)+'\n')
print('Final native source and artifact identities verified after completed tests')
