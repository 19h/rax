"""Verify the independent native matrix without treating it as guest proof."""
from pathlib import Path
import hashlib,json,re
ROOT=Path(__file__).resolve().parent
PROFILES=['arm64','x64','x86','x86-laa']
FIELDS=['kind','role','length','status','output-bytes-cells','return-bytes-cells','output-protect','returned-protect']
def need(x,m):
 if not x:raise ValueError(m)
def load(n):return json.loads((ROOT/n).read_text(encoding='utf-8-sig'))
def main():
 need(__debug__,'optimized verification rejected')
 inputs=load('inputs.json');seen=set()
 for r in inputs['files']:
  n=r['path'];need(n not in seen and not Path(n).is_absolute() and '..' not in Path(n).parts,'unique relative evidence path');seen.add(n)
  b=(ROOT/n).read_bytes();need((len(b),hashlib.sha256(b).hexdigest())==(r['bytes'],r['sha256']),n)
 raw=(ROOT/'cases.json').read_bytes();need(hashlib.sha256(raw).hexdigest()==inputs['cases-sha256'],'canonical cases bytes')
 canonical=load('cases.json');need(canonical['profiles']==PROFILES and canonical['per-profile']==1830 and len(canonical['cases'])==1830,'complete canonical matrix')
 need(load('token-query-v5-native-launch.json')['exit-code']==0,'native launcher finished')
 identities=load('native-token-query-v5-identities.json')
 need(identities['calls']==7320 and identities['closed-handles']==12 and identities['released-arenas']==4,'complete native cleanup')
 outcomes=load('native-token-query-v5-outcomes.json')
 need([x['profile'] for x in outcomes]==PROFILES and all(x['exit-code']==0 for x in outcomes),'four native producers finished')
 for profile,idn,outcome in zip(PROFILES,identities['producers'],outcomes):
  need(idn['profile']==profile and idn['calls']==1830,'inspected profile')
  need(idn['source']['sha256']==inputs['source-sha256']==outcome['source-hash'].lower(),'compiled native source')
  need(idn['executable']['sha256']==outcome['exe-sha256'].lower(),'actual native executable identity')
  need(idn['large-address-aware']==(profile!='x86'),'actual LAA profile')
  log=ROOT/('native-token-query-v5-'+profile+'.log');raw=log.read_bytes();need(hashlib.sha256(raw).hexdigest()==idn['stdout']['sha256'],'independent stdout identity')
  lines=raw.decode().splitlines()
  need(lines[0]==f'identity profile={profile} pointer-bytes={4 if profile.startswith("x86") else 8} build=10.0.29683 page=4096 cases=1830','native profile and kernel')
  need(lines[-1]=='complete cases=1830 closed-handles=3 released-arenas=1','native cleanup')
  rows=[dict(re.findall(r'([\w-]+)=([^\s]*)',x)) for x in lines if x.startswith('case ')]
  need([int(x['ordinal']) for x in rows]==list(range(1830)),'complete ordered rows')
  need(all(x['escaped']=='00000000' and x['last-error']=='7E11ABCD' for x in rows),'SEH and last-error outcomes')
  need([{k:x[k] for k in FIELDS} for x in rows]==canonical['cases'],'native semantic rows agree '+profile)
 kinds=['primary','thread-token','effective','null','process','thread','invalid','event','real','no-access']
 lengths=[0,1,2,3,4,5,8,15,16,4096,0xFFFFFFFF]
 expected=[(kind,'0',f'{length:08X}') for kind in kinds for length in lengths]
 expected += [(kind,str(role),f'{length:08X}') for kind in kinds for role in range(43) for length in [0,4,8,0xFFFFFFFF]]
 need([(r['kind'],r['role'],r['length']) for r in canonical['cases']]==expected,'complete ordered length/pointer matrix; forty repeated controls are intentional')
 print('Verified7320 native calls, four inspected producers, retained byte/protection effects and cleanup; guest validation is separate')
if __name__=='__main__':main()
