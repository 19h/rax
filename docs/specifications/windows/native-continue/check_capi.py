"""Audit the retained Assist adapter/C ABI ordinary-program continuation."""
from pathlib import Path
import hashlib,json
D=Path(__file__).resolve().parent;V=D/'validation'
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def bound(row,p):
 raw=p.read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256'])
def check():
 value=load(V/'continue-owning-capi-b.json');assert value['state']=='finished'and value['exit-code']==0
 for name in['source','carrier','adapter-inputs','executable','programs','runtime','native']:assert value[name+'-before']==value[name+'-after']
 source=load(V/'continue-owning-capi-source.json');bound(source,V/'continue-owning-capi.cpp');assert(source['bytes'],source['sha256'])==(value['source-before']['bytes'],value['source-before']['sha256'])
 inputs=load(V/'owning-capi-inputs.json')['files'];assert len(inputs)==len(value['adapter-inputs-before'])==4
 for row,record in zip(inputs,value['adapter-inputs-before']):
  assert len(row['repository-commit'])==len(row['git-blob'])==40
  assert(row['bytes'],row['sha256'])==(record['bytes'],record['sha256'])and record['path'].replace('\\','/').endswith('/'+row['role'])
 header=next(x for x in load(V/'source-freeze.json')['files']if x['path']=='capi/include/rax.h');assert(header['bytes'],header['sha256'])==(inputs[-1]['bytes'],inputs[-1]['sha256'])
 carrier=next(x for x in load(V/'archives-windows-after.json')['archives']if x['path'].endswith('.lib'));assert(carrier['bytes'],carrier['sha256'])==(value['carrier-before']['bytes'],value['carrier-before']['sha256'])
 assert[x['name']for x in value['steps']]==['build','ordinary']
 for row in value['steps']:assert row['exit-code']==0;bound(row,V/row['log'])
 text=(V/value['steps'][1]['log']).read_text();records=[json.loads(x[len('api-result '):])for x in text.splitlines()if x.startswith('api-result ')];assert records==value['outcomes']and len(records)==2
 for index,row in enumerate(records):
  assert row['program']==value['programs-before'][index]['path']and row['native-runtime']is True and row['slice-instructions']==4096 and row['turns-started']>0 and row['elapsed-us']>0
  assert value['native-before'][index]['exit-code']==0 and row['stderr-hex']==''
 first,second=records;assert first['reason']==4 and first['exit-code']==0 and first['stdout-hex']==''and value['native-before'][0]['stdout-hex']==''
 assert second['reason']==5 and'NtQueryVirtualMemory class 3'in json.dumps(second['diagnostic'])
 assert len(value['runtime-before'])==6 and value['runtime-before']==load(V/'ordinary.json')['runtime-before']
 return{'scope':'Retained Assist ABI184 adapter/C ABI/actual carrier; distinct from current-root whole GUI/plugin/package proof.','smoke-exit-reason':4,'smoke-exit-code':0,'smoke-elapsed-us':first['elapsed-us'],'smoke-turns':first['turns-started'],'slice-instructions':4096,'whoami-frontier':'NtQueryVirtualMemory class3'}
if __name__=='__main__':print(json.dumps(check(),indent=2))
