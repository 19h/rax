from pathlib import Path
import json,re,shutil,hashlib
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
def load(p):
 b=p.read_bytes();return json.loads(b.decode('utf-16')if b[:2]in[b'\xff\xfe',b'\xfe\xff']else b.decode('utf-8-sig'))
launch=load(S/'directory-coherence-v2-launch.json');assert launch['state']=='finished'and launch['exit_code']==0
rows=load(S/'directory-coherence-v2-outcomes.json');assert [r['profile']for r in rows]==['arm64','x64','x86','x86-laa']
source_hash=hashlib.sha256((B/'native-directory-coherence-v2.cpp').read_bytes()).hexdigest();results=[]
for row in rows:
 profile=row['profile'];prefix='directory-coherence-v2-'+profile
 assert row['exit-code']==0 and row['source']==source_hash
 pre=load(S/(prefix+'-before.json'));post=load(S/(prefix+'-after.json'));assert pre==post
 assert pre['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[profile]
 assert pre['large-address-aware']==(profile!='x86')
 text=(S/row['log']).read_text(encoding='utf-8-sig');passes=[]
 for i in range(6):
  entries=re.findall(r'^entry pass='+str(i)+r' ordinal=(\d+) context=(\d+) name=([0-9A-F]+) type=([0-9A-F]+)$',text,re.M)
  assert [int(r[0])for r in entries]==list(range(len(entries)))
  entries=sorted((r[2],r[3])for r in entries);unique=len({r[0]for r in entries})==len(entries)
  summary=re.search(r'^pass-complete pass='+str(i)+r' count=(\d+) unique=(\d+) same-previous=(\d+)$',text,re.M);assert summary
  assert tuple(map(int,summary.groups()))==(len(entries),int(unique),int(i>0 and entries==passes[-1]['entries']))
  passes.append({'entries':entries,'unique':unique})
 pairs=[]
 for attempt in range(3):
  a,b=passes[attempt*2:attempt*2+2];coherent=a['unique']and b['unique']and a['entries']==b['entries']
  match=re.search(r'^pair attempt='+str(attempt)+r' coherent=(\d+)$',text,re.M);assert match and int(match[1])==int(coherent);pairs.append(coherent)
 complete=re.search(r'^complete passes=6 bounded-acquisition=(\w+) handles-before=(\d+) handles-after=(\d+)$',text,re.M);assert complete
 assert complete[1]==('accepted'if any(pairs)else'unstable')and complete[2]==complete[3]
 for name in [row['log'],prefix+'-before.json',prefix+'-after.json',prefix+'-build.log']:shutil.copyfile(S/name,B/name)
 results.append({'profile':profile,'passes':6,'entry-rows':sum(len(p['entries'])for p in passes),'pairs':pairs,'handles-before':int(complete[2]),'handles-after':int(complete[3]),'pe':pre})
for name in ['directory-coherence-v2-launch.json','directory-coherence-v2-outcomes.json','directory-coherence-v2-driver.log']:shutil.copyfile(S/name,B/name)
(B/'coherence-v2-summary.json').write_text(json.dumps(results,indent=2)+'\n')
print('Verified',sum(r['passes']for r in results),'enumerations and',sum(r['entry-rows']for r in results),'entry rows; pair outcomes',[(r['profile'],r['pairs'])for r in results])
