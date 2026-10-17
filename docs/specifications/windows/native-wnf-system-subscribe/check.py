"""Check System WNF native identities, cleanup and exact replay fixtures."""
from pathlib import Path
import hashlib,json
D=Path(__file__).resolve().parent
def load(name):return json.loads((D/name).read_text(encoding='utf-8-sig'))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def need(x,m):
 if not x:raise ValueError(m)
def rows(p,kind='case '):
 return [dict(x.split('=',1)for x in line.split()[int(kind=='case '):])for line in p.read_text(encoding='utf-8-sig').splitlines()if line.startswith(kind)]
def main():
 need(__debug__,'optimized verification refused')
 seen=set()
 for r in load('inputs.json')['files']:
  p=r['path'];need(p not in seen and not Path(p).is_absolute()and '..'not in Path(p).parts,'unique relative path');seen.add(p)
  need((D/p).stat().st_size==r['bytes']and sha(D/p)==r['sha256'],p)
 c=load('cases.json');ids=load('native/v3/native-system-wnf-subscribe-v3-identities.json')['producers']
 need(len(ids)==4,'four actual producers')
 for r,(profile,machine)in zip(ids,[('arm64',0xaa64),('x64',0x8664),('x86',0x14c),('x86-laa',0x14c)]):
  p=D/'native/v3'/('native-system-wnf-subscribe-v3-'+profile+'.log')
  need(r['profile']==profile and r['executable']['machine']==machine,'producer architecture')
  need(r['source-sha256']==sha(D/'native/native-system-wnf-subscribe-v3.cpp'),'producer source')
  need(r['stdout']['sha256']==sha(p)and r['stdout']['bytes']==p.stat().st_size,'producer stdout')
  rr=rows(p);need(len(rr)==768 and rr==c['permissions'][profile],'exact permission rows')
  need('complete cases=768 created=8 deleted=8 freed=8 subscriptions=279 unsubscribed=279 failed=0'in p.read_text(),'producer cleanup')
 p=D/'native/native-system-wnf-subscribe-deny-v3.log';r=load('native/native-system-wnf-subscribe-deny-v3-verified.json')
 need(r['source-sha256']==sha(D/'native/native-system-wnf-subscribe-deny-v3.cpp')and r['stdout-sha256']==sha(p)and r['stdout-bytes']==p.stat().st_size and r['machine']==0xaa64,'ordered deny producer identity')
 rr=rows(p)
 for row in rr:row['descriptor']=str(int(row['descriptor'])+8)
 need(len(rr)==384 and rr==c['permissions']['arm64-deny'],'exact ordered deny rows')
 need('complete cases=384 created=4 deleted=4 freed=4 subscriptions=144 unsubscribed=144 failed=0'in p.read_text(),'ordered deny cleanup')
 fresh=load('native/native-system-wnf-subscribe-fresh-verified.json');owned=load('native/native-system-wnf-subscribe-owned-fresh-verified.json')
 need(c['fresh'][0]['rows']==fresh['rows'],'selected System fixture')
 for i,run in enumerate(owned['runs']):need(c['fresh'][i+1]['rows']==run['rows'],'owned System fixture')
 for stem,record,key in [('native-system-wnf-subscribe-fresh',fresh,None),('native-system-wnf-subscribe-transitions',load('native/native-system-wnf-subscribe-transitions-verified.json'),'transitions'),('native-system-wnf-subscribe-triples-v2',load('native/native-system-wnf-subscribe-triples-v2-verified.json'),'triples')]:
  p=D/'native'/(stem+'.log');need(record['source']['sha256']==sha(D/'native'/(stem+'.cpp'))and record['stdout']['sha256']==sha(p),'fresh/transition source and stdout')
  rr=rows(p,'case=');need(rr==record['rows']and len(rr)==record['cases']+1,'all measured rows plus warmup')
  need(all(r['exit']=='77'and (r['warmup']=='1'or r['handles-before']==r['handles-after'])for r in rr),'owned child cleanup')
  need(record['selected-before-after']=={'stamp':1,'data':'0100000000000000'},'existing selected state unchanged')
  if key:need(rr==c[key],'exact transition fixture')
 for run in owned['runs']:
  p=D/'native'/('native-system-wnf-subscribe-owned-fresh-'+run['mode']+'.log')
  need(run['stdout']['sha256']==sha(p)and run['rows']==rows(p,'case='),'owned fresh source rows')
  need(run['created']==run['deleted']==run['freed-descriptors']==1,'owned name and descriptor cleanup')
 need(owned['source']['sha256']==sha(D/'native/native-system-wnf-subscribe-owned-fresh.cpp'),'owned producer source')
 zero=load('native/native-system-wnf-subscribe-zero-fresh-v2-verified.json');run=zero['runs'][0]
 p=D/'native/native-system-wnf-subscribe-zero-fresh-v2-value.log'
 need(zero['source']['sha256']==sha(D/'native/native-system-wnf-subscribe-zero-fresh-v2.cpp')and run['stdout']['sha256']==sha(p),'zero-length producer')
 need(run['rows']==c['fresh'][3]['rows']and len(run['rows'])==193,'zero-length fresh fixture')
 need('selected-size-before=0'in p.read_text()and 'selected-size-after=0'in p.read_text(),'actual zero-length query capture')
 need(run['created']==run['deleted']==run['freed-descriptors']==1,'zero-length cleanup')

 recheck=load('native/system-wnf-deny-cleanup-recheck.json');need(recheck['query-status']==0 and recheck['exists']==0,'failed explicit delete name disappeared with creator')
 need('C4473'in (D/'native/native-system-wnf-subscribe-triples-build.log').read_text(),'retain initial triple producer failure')
 need('LNK1181'in (D/'native/native-system-wnf-subscribe-deny-build.log').read_text(),'retain initial deny runner failure')
 print('Verified3456 native permission rows,1344 fresh/transition cases and owned-resource cleanup')
if __name__=='__main__':main()
