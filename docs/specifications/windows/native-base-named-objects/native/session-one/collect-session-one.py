from pathlib import Path
import json,re,hashlib,shutil
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
def load(p):
 raw=p.read_bytes();return json.loads(raw.decode('utf-16')if raw[:2]in [b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
launch=load(S/'session-one-directories-v1-launch.json')
assert launch['state']=='finished'and launch['exit_code']==0
rows=load(S/'session-one-directories-v1-outcomes.json');assert [r['profile']for r in rows]==['arm64','x64','x86','x86-laa'];report=[]
for row in rows:
 profile=row['profile'];assert row['exit-code']==0 and row['source']==hashlib.sha256((B/'native-session-one-directories-v1.cpp').read_bytes()).hexdigest()
 prefix='session-one-directories-v1-'+profile
 before,after=[load(S/(prefix+'-'+name+'.json'))for name in ['before','after']];assert before==after
 assert before['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[profile]
 assert before['large-address-aware']==(profile!='x86')
 text=(S/row['log']).read_text(encoding='utf-8-sig');assert 'selection-session=1 source=observed-NT-Sessions-directory' in text
 calls=re.findall(r'^open path=(\d+) flags=([0-9A-F]+) access=([0-9A-F]+) status=([0-9A-F]+)',text,re.M)
 assert len(calls)==len(set((a,b,c)for a,b,c,_ in calls))==384
 complete=re.findall(r'^complete paths=(\d+) opens=(\d+) entries=(\d+) handles-before=(\d+) handles-after=(\d+)$',text,re.M)
 assert len(complete)==1 and complete[0][0]=='6'and complete[0][3]==complete[0][4]
 assert int(complete[0][1])==sum(int(status,16)<0x80000000 for *_,status in calls)
 entries=re.findall(r'^entry path=(\d+) ordinal=(\d+) context=(\d+) name=([0-9A-F]*) type=([0-9A-F]*)$',text,re.M)
 assert len(entries)==int(complete[0][2]);paths=[{'ordinal':int(i),'name':bytes.fromhex(name).decode('utf-16-le')}for i,name in re.findall(r'^path ordinal=(\d+) name=([0-9A-F]+)$',text,re.M)]
 assert [p['ordinal']for p in paths]==list(range(6))
 for name in [prefix+'-before.json',prefix+'-after.json',row['log'],prefix+'-build.log']:shutil.copyfile(S/name,B/name)
 report.append({'profile':profile,'pe':before,'identity':text.splitlines()[0],'paths':paths,'matrix-opens':len(calls),'entries':len(entries),'handles-before':int(complete[0][3]),'handles-after':int(complete[0][4])})
for name in ['session-one-directories-v1-launch.json','session-one-directories-v1-outcomes.json','session-one-directories-v1-driver.log']:shutil.copyfile(S/name,B/name)
(B/'native-session-one-summary.json').write_text(json.dumps({'completed-profiles':report,'complete':True},indent=2)+'\n')
print('Verified four native directory profiles,',sum(r['matrix-opens']for r in report),'matrix opens, balanced private handles and unchanged executables')
