from pathlib import Path
import hashlib,json,re,shutil,subprocess,tempfile
N=Path(__file__).resolve().parent;D=N/'wait-source/docs/specifications/windows/native-single-wait';R=N/'wait-source'
def load(path):return json.loads(path.read_text())
def write(path,value):path.write_text(json.dumps(value,indent=2)+'\n')
def gate(root,host,name,before,after):
 p=root/'validation'/('wait-native-epoch4-gates.json'if host=='windows'else'wait-'+host+'-epoch4-gates.json');data=load(p);row=next(r for r in data['steps']if r['name']==name);log=root/'validation'/row['log'];text=log.read_text();assert before in text;text=text.replace(before,after);log.write_text(text);raw=log.read_bytes();row.update({'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'results':re.findall(r'test result: ([^\r\n]+)',text)});write(p,data)
def json_field(root,name,mutate):
 p=root/'validation'/name;data=load(p);mutate(data);write(p,data)
def cpp_archive(root):
 p=root/'validation/wait-native-owning-epoch4-cpp.log';text=p.read_text();lines=[]
 for line in text.splitlines():
  if line.startswith(('linked-archive-before ','linked-archive-after ')):
   prefix,raw=line.split(' ',1);data=json.loads(raw);data['sha256']='0'*64;line=prefix+' '+json.dumps(data)
  lines.append(line)
 p.write_text('\n'.join(lines)+'\n')
def ordinary(root):
 p=root/'validation/ordinary-program-summary.json';data=load(p);row=data['runs'][0];old=row['native-wait-result-line'];new=old.replace('status=Some(0)','status=Some(c0)');row['native-wait-result-line']=new;write(p,data)
 log=root/'validation'/row['log'];log.write_text(log.read_text().replace(old,new));p=root/'validation/wait-owner-epoch4-launch.json';launch=load(p);launch['outcomes'][0]['log-identity']['sha256']=hashlib.sha256(log.read_bytes()).hexdigest();write(p,launch)
def rebind(root):
 p=root/'evidence.json';data=load(p)
 for row in data['files']:
  raw=(root/row['path']).read_bytes();row.update({'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
 write(p,data)
cases=[
 ('focused-selection',lambda r:gate(r,'macos','focus','16 passed; 0 failed;','0 passed; 0 failed;')),
 ('full-filtered-out',lambda r:gate(r,'macos','full','0 filtered out;','1 filtered out;')),
 ('capi-selection',lambda r:gate(r,'macos','capi','168 passed; 0 failed;','0 passed; 0 failed;')),
 ('native-integration-selection',lambda r:gate(r,'windows','integration','4 passed; 0 failed;','0 passed; 0 failed;')),
 ('existing-windows-failure-set',lambda r:json_field(r,'wait-native-epoch4-gates.json',lambda d:d['exact-existing-failures'].__setitem__(0,'unexpected::new_failure'))),
 ('core-features',lambda r:json_field(r,'macos-wait-epoch4-final-owning-archive-identities.json',lambda d:d.__setitem__('core-features',['smir-jit']))),
 ('archive-member-walk',lambda r:json_field(r,'linux-wait-epoch4-final-owning-archive-identities.json',lambda d:d['archives'][0].__setitem__('members',0))),
 ('current-root-claim',lambda r:json_field(r,'consumer-summary.json',lambda d:d.__setitem__('whole-current-root-product-validated',True))),
 ('linked-native-carrier',cpp_archive),
 ('ordinary-native-status',ordinary),
 ('compiled-source',lambda r:json_field(r,'wait-source-epoch4.json',lambda d:next(v for v in d['files']if v['path']=='src/user/windows/dll/native.rs').__setitem__('sha256','0'*64))),
 ('program-identity',lambda r:json_field(r,'wait-owner-epoch4-launch.json',lambda d:[d['outcomes'][0][k].__setitem__('sha256','0'*64)for k in ['program-before','program-after']])),
]
results=[]
for name,mutate in cases:
 with tempfile.TemporaryDirectory(prefix='assist-wait-validation-stress-')as temporary:
  root=Path(temporary)/'proof';shutil.copytree(D,root,ignore=shutil.ignore_patterns('__pycache__'));mutate(root);rebind(root)
  p=subprocess.run(['python3','-B',str(root/'check_validation.py'),'--source-root',str(R)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  assert p.returncode!=0 and 'ValueError:'in p.stdout,(name,p.stdout)
  reason=p.stdout.strip().splitlines()[-1];results.append({'name':name,'result':'rejected','outer-digests-rebound':True,'reason':reason});print(name,reason,flush=True)
(N/'wait-final-stress.json').write_text(json.dumps({'source-epoch':4,'cases':results},indent=2)+'\n')
print('Rejected',len(results),'rebound final-evidence corruptions')
