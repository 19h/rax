from pathlib import Path
import json,hashlib,os,datetime
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
def need(x,m):
 if not x:raise ValueError(m)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
prior=json.loads((S/'access-check-initial-launch.json').read_text());need(prior.get('state')=='finished' and prior.get('exit-code')==1,'initial native run completed with retained fixture failures')
g=json.loads((S/'native-access-check-initial-gates.json').read_text(encoding='utf-8-sig'));need(len(g)==4 and [x['exit-code'] for x in g]==[101,0,0,0],'expected initial gate statuses')
config=B/'src/.cargo/config.toml';need(sha(config)=='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c','preserve Cargo adapter')
d=json.loads((S/'access-check-epoch2-delta.json').read_text());p=B/'src'/d['path'];need(sha(p)==d['previous-sha256'],'old test source')
raw=d['text'].encode();need(len(raw)==d['bytes'] and hashlib.sha256(raw).hexdigest()==d['sha256'],'new test source')
t=p.with_name(p.name+'.access-epoch2-temporary');need(not t.exists(),'no earlier temporary source');t.write_bytes(raw);os.replace(t,p)
m=json.loads((S/'access-check-epoch2-owning-source-hashes.json').read_text());need(len(m['sources'])==319,'319 source inputs')
for r in m['sources']:need(sha(B/'src'/r['path'])==r['sha256'],'source '+r['path'])
r={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'base':'23ac34c5d0d0f6afff762431442d2fab52cda7f9','changed':{k:v for k,v in d.items() if k!='text'},'selected-source-count':319,'scope':'test caller stack and exact unsupported assertions only; production code unchanged'}
(S/'native-access-check-epoch2-source-verification.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
