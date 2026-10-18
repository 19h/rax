from pathlib import Path
import json,hashlib,shutil,re
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
records=[]
for prefix in ['directory-frontier','directory-fast']:
 value=json.loads((S/(prefix+'-launch.json')).read_text());assert value['state']=='finished'and value['exit_code']==0
 assert value['source-before']==value['source-after']and value['executable-before']==value['executable-after']
 for i,row in enumerate(value['outcomes']):
  assert row['exit-code']==0 and row['program']==row['program-after']
  p=S/row['log'];raw=p.read_bytes();assert len(raw)==row['log-identity']['bytes']and hashlib.sha256(raw).hexdigest()==row['log-identity']['sha256']
  text=raw.decode('utf-8-sig');turn=[2149956,2152305][i]
  assert f'terminal turn={turn} Complete(Internal('in text and 'NT directory outside selected installed namespace'in text
  if prefix=='directory-fast':
   assert 'prefix-turns=2100000 result=BudgetExhausted'in text
   assert f'directory turn={turn} access=0xf root=Ok(0) flags=Ok(40)'in text
   assert 'directory name="\\\\BaseNamedObjects" length=34'in text
  else:assert 'decoded=Some("\\\\BaseNamedObjects")'in text
  shutil.copyfile(p,B/p.name)
  records.append({'observer':prefix,'program-sha256':row['program']['sha256'],'turn':turn,'nt-name':'\\BaseNamedObjects','log':p.name,'log-sha256':row['log-identity']['sha256'],'terminal':'unimplemented selected namespace, not a successful guest exit'})
 for name in [prefix+'-launch.json',prefix+'-build.log',prefix+'-build.ps1',prefix+'-trace.rs','run-'+prefix+'.py']:
  if(S/name).exists():shutil.copyfile(S/name,B/name)
(B/'diagnostic-summary.json').write_text(json.dumps(records,indent=2)+'\n')
print('Verified both programs under both diagnostic observers')
