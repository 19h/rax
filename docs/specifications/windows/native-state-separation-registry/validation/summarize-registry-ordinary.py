from pathlib import Path
import hashlib,json,re,shutil
Q=Path(__file__).parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D=Q/'registry-source/docs/specifications/windows/native-state-separation-registry';V=D/'validation'
r=json.loads((S/'redirection-owner-epoch1-launch.json').read_text())
assert r['state']=='finished' and r['exit_code']==0
owner=json.loads((S/'native-registry-epoch1-final-owning-archive-identities.json').read_text())
core=next(row for row in owner['archives'] if row['path'].endswith('.rlib'))
assert r['archive']['sha256']==core['sha256']==r['archive-after']['sha256']
assert r['source-before']==r['source-after'] and r['executable-before']==r['executable-after']
assert r['runtime-modules-before']==r['runtime-modules-after']
assert r['source-before']['sha256']=='f73f278d4b8e53417453c4cbe06a6eead338710653e5249a0d35d96c30be133a'
rows=[]
for index,(old_stop,expected_sha) in enumerate([(2178927,'325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b'),(2181212,'48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6')]):
 outcome=r['outcomes'][index];assert outcome['exit-code']==0 and outcome['program-before']==outcome['program-after']
 assert outcome['program-before']['sha256']==expected_sha
 raw=(S/outcome['log']).read_bytes();assert hashlib.sha256(raw).hexdigest()==outcome['log-identity']['sha256']
 text=raw.decode('utf-8')
 assert f'registry-result turn={old_stop} tid=8 status=Some(c0000034) output=Ok(0) result=BudgetExhausted' in text
 assert 'prefix-turns=2100000 result=BudgetExhausted' in text
 end=re.search(r'terminal turn=(\d+) Complete\(Internal\("([^"]+)"\)\)',text);assert end,text[-1500:]
 terminal=int(end[1]);assert terminal>old_stop
 assert 'NtSetTimer2 (0x1c0)' in end[2]
 rows.append({'program':outcome['program-before'],'former-stop-turn':old_stop,'new-terminal-turn':terminal,'additional-observer-turns':terminal-old_stop,'registry-ntstatus':'C0000034','output-handle':0,'guest-exit-success':False,'terminal':end[2],'log':outcome['log'],'log-sha256':outcome['log-identity']['sha256'],'controller-exit-code':0})
 for base in [Q,V]:shutil.copy2(S/outcome['log'],base/outcome['log'])
summary={'source-epoch':1,'actual-core':core,'source':r['source-before'],'runtime-module-identity-preserved':True,'ordinary-guest-exit-success':False,'programs':rows,'scope':'The complete instruction prefix executes normally. The observed missing registry result matches native capture; no syscall result substitution. Controller exit zero does not imply successful guest exit.'}
for base in [Q,V]:(base/'ordinary-program-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
for name in ['redirection-owner-epoch1-launch.json','redirection-owner-epoch1-dispatch.json','redirection-owner-epoch1-controller.log','redirection-owner-epoch1-build.log']:
 for base in [Q,V]:shutil.copy2(S/name,base/name)
print(json.dumps(rows,indent=2))
