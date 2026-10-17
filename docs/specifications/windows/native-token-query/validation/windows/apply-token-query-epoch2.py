from pathlib import Path
import json,hashlib,os,datetime
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');B=Path('C:/Windows/Temp/assist-native-rax-20261010')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def need(x,m):
 if not x:raise ValueError(m)
old=json.loads((S/'token-query-final-launch.json').read_text());need(old.get('exit-code')==1 and old.get('core-exit-code')==101,'initial native tests finished with retained compile failure')
config=B/'src/.cargo/config.toml';need(sha(config)=='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c','core Cargo adapter preserved')
r=json.loads((S/'token-query-epoch2-delta.json').read_text());p=B/'src'/r['path'];need(sha(p)==r['previous-sha256'],'old native test input');data=r['text'].encode();need(len(data)==r['bytes'] and hashlib.sha256(data).hexdigest()==r['sha256'],'new test input')
tmp=p.with_name(p.name+'.epoch2-temporary');need(not tmp.exists(),'no previous partial write');tmp.write_bytes(data);os.replace(tmp,p)
m=json.loads((S/'token-query-epoch2-owning-source-hashes.json').read_text());need(len(m['sources'])==313,'source count')
for i in m['sources']:need(sha(B/'src'/i['path'])==i['sha256'],'current source '+i['path'])
out={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'base':'aa9bcc5651164ade3265e124de9a6d4f778f561f','changed':{k:v for k,v in r.items() if k!='text'},'selected-source-count':313,'scope':'Windows-only test module reference; production code unchanged and original observations retained'}
(S/'native-token-query-epoch2-source-verification.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out))
