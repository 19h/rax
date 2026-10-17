"""Verify native WNF query evidence independently of the guest implementation."""
from pathlib import Path
import hashlib,itertools,json,re,struct
R=Path(__file__).resolve().parent
PROFILES=['arm64','x64','x86','x86-laa']
MACHINES={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}
def need(value,message):
 if not value:raise ValueError(message)
def load(name):return json.loads((R/name).read_text(encoding='utf-8-sig'))
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def rows(path):
 return [dict(pair.split('=',1) for pair in line.split()[1:]) for line in path.read_text(encoding='utf-8-sig').splitlines() if line.startswith('case ')]
def canonical(row):return {k:v for k,v in row.items() if k not in ['ordinal','escaped','last-error']}
def inspect(family,prefix,profiles,count,deleted,descriptors):
 identities=load('native/'+family+'/'+prefix+('-partial-identities.json' if family=='v5' else '-identities.json'))
 need(len(identities['producers'])==len(profiles),'all inspected producer profiles')
 result={}
 for profile,identity in zip(profiles,identities['producers']):
  need(identity['profile']==profile and identity['machine']==MACHINES[profile] and identity['large-address-aware']==(profile!='x86'),'actual native producer ABI')
  source=R/'native'/family/(prefix+'.cpp');output=R/'native'/family/(prefix+'-'+profile+'.log')
  need(sha(source)==identity['source']['sha256'],'actual compiled source identity')
  need((output.stat().st_size,sha(output))==(identity['stdout']['bytes'],identity['stdout']['sha256']),'actual stdout identity')
  rr=rows(output);text=output.read_text(encoding='utf-8-sig')
  need(len(rr)==count and [int(r['ordinal']) for r in rr]==list(range(count)),'complete native ordinal sequence')
  need(all(r['escaped']=='00000000' and r['last-error']=='7E11ABCD' for r in rr),'native exception and last error records')
  need(identity['calls']==count and identity['escaped-exceptions']==0,'inspected native count')
  need(f'complete cases={count} deleted-states={deleted} released-arenas=1 released-descriptors={descriptors} closed-handles=1' in text,'actual resource cleanup')
  need([line for line in text.splitlines() if line.startswith('delete ')]==[f'delete index={i} status=00000000' for i in range(deleted)],'all private state deletions')
  result[profile]=rr
 return result

def main():
 need(__debug__,'optimized verification rejected')
 seen=set()
 for item in load('inputs.json')['files']:
  name=item['path'];need(name not in seen and not Path(name).is_absolute() and '..' not in Path(name).parts,'unique relative evidence')
  seen.add(name);p=R/name;need((p.stat().st_size,sha(p))==(item['bytes'],item['sha256']),name)
 fixture=load('cases.json')
 v3=inspect('v3','native-wnf-query-v3',PROFILES,1920,3,1)
 v5=inspect('v5','native-wnf-query-v5',['arm64','x86','x86-laa'],2160,4,2)
 expected=list(itertools.product(['owned-empty','owned-payload','owned-typed','owned-denied','known-process','captured-system','zero','all-ones','unselected'],range(5),range(4),[0,1,3,4,7,8,15,16,17,64,4096,0xFFFFFFFF]))
 for profile,rr in v5.items():
  need([(r['name'],int(r['type']),int(r['scope']),int(r['capacity'],16)) for r in rr]==expected,'complete V5 Cartesian matrix')
  need([canonical(r) for r in rr]==fixture['logical'],'verified V5 logical fixture')
  need(sum(r['name']=='owned-denied' and r['return']=='C0000022' for r in rr)==55,'read-denied native controls')
 for profile,rr in v3.items():
  need(len([r for r in rr if r['name']=='captured-system' and r['type']=='4' and r['scope']=='0' and r['capacity']=='00001000' and r['return']=='00000000' and r['stamp']=='00000001' and r['length']=='00000008' and r['bytes'].startswith('0100000000000000')])==1,'actual typed continuation value')
  need([canonical(r) for r in rr]==[canonical(r) for r in v3['arm64']],'four V3 profiles agree')
 boundary=inspect('boundaries','native-wnf-query-boundaries',PROFILES,1560,3,1)
 expected=list(itertools.product(['owned-empty','owned-payload','captured-system','zero'],[0,1,8,16,64,0xFFFFFFFF],range(65)))
 for profile,rr in boundary.items():
  need([(r['name'],int(r['capacity'],16),int(r['role'])) for r in rr]==expected,'complete boundary Cartesian matrix')
  need([canonical(r) for r in rr]==fixture['boundaries'][profile],'all captured pointer bytes retained for '+profile)
  need(all(re.fullmatch(r'(?:[0-9A-F]{2}|\?\?){8}',r[f'p{i}-cells']) for r in rr for i in range(6)),'eight byte cells per pointer')
 # V5 x64 finished its native calls, but Defender quarantined its executable
 # before the independent PE/hash walk. It is not an inspected producer above.
 raw_x64=rows(R/'native/v5/native-wnf-query-v5-x64.log')
 need(len(raw_x64)==2160 and [canonical(r) for r in raw_x64]==fixture['logical'],'retained but unverified V5 x64 corroboration')
 need(load('native/v5/wnf-query-v5-native-launch.json')['exit-code']==1,'retain the incomplete first V5 driver')
 need(load('native/v5/native-wnf-query-v5-partial-identities.json')['x64-identity']=='pending user review of Defender quarantine','explicit x64 identity boundary')
 controls=load('controls/wnf-private-scope-order.json')
 need(len(controls['rows'])==168 and len(controls['created'])==8 and len(controls['cleanup'])==11,'complete private scope control count')
 need(all(r.get('status',0)==0 and r.get('ok',True) is True for r in controls['cleanup']),'private scope cleanup')
 need(controls['source-sha256']==sha(R/'controls/probe-private-scope-order.py'),'private scope source identity')
 for row in controls['rows']:
  if row['denied'] and row['explicit']=='null':need(row['status']==0xC0000022,'denial precedes TypeId mismatch')
 rights=load('controls/wnf-process-scope-rights-v2.json')
 need(rights['closed-handles']==6 and [x['access'] for x in rights['rows']]==[0,1,16,1024,4096,2097151]
      and all(x['status']==0 and x['stamp']==0 and x['length']==0 and x['last-error']==0x7E11ABCD for x in rights['rows']), 'zero-right process scope handles')
 need(rights['source-sha256']==sha(R/'controls/probe-process-scope-rights-v2.py'),'process-scope rights producer')
 cross=load('controls/wnf-owned-cross-scope.json')
 need(cross['source-sha256']==sha(R/'controls/probe-owned-cross-scope.py'),'cross-scope source identity')
 need(len(cross['rows'])==10 and len(cross['cleanup'])==3 and all(x.get('status',0)==0 and x.get('ok',True)is True for x in cross['cleanup']),'cross-scope complete cleanup')
 need(all(x['status']==0xC0000034 for x in cross['rows']if x['op']in ['query','update']and x['target']=='other'),'native valid unselected namespace counterexample')
 capture=load('capture/captured-query.json')
 need(sha(R/'capture/capture.log')==capture['trace-sha256'] and sha(R/'capture/native-wnf-query-trace.rs')==capture['source-sha256'],'ordinary input trace identity')
 need(sha(R/'primary/phnt-ntexapi.h')==load('primary/phnt-source.json')['sha256'],'pinned primary declaration')
 print('Verified WNF native logical/protection matrices, inspected producer identities, private scope cleanup and explicit unverified x64 V5 boundary')
if __name__=='__main__':main()
