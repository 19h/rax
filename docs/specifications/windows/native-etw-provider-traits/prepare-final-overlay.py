from pathlib import Path
import subprocess,json,hashlib,zipfile
base=Path(__file__).parent
prior=Path('/Users/int/Library/Caches/assist-process-emulation/wnf-process-event-20261012')
root=Path('/Users/int/hexrays/kvasir');rax=root/'vendor/rax'
share=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
old=json.loads((Path('/Users/int/Library/Caches/assist-process-emulation/etw-register-guids-20261012')/'native-etw-register-guids-current-overlay-manifest.json').read_text())
previous={(r['repo'],r['path']):r['sha256'] for r in old['source-files']}
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
root_bytes={}
entries=subprocess.check_output(['git','ls-tree','-r','-z',head],cwd=root).split(b'\0')
batch=subprocess.Popen(['git','cat-file','--batch'],cwd=root,stdin=subprocess.PIPE,stdout=subprocess.PIPE)
for entry in filter(None,entries):
 info,name=entry.split(b'\t',1);mode,kind,oid=info.split();name=name.decode()
 if kind!=b'blob' or name.startswith('vendor/'):continue
 batch.stdin.write(oid+b'\n');batch.stdin.flush();header=batch.stdout.readline().split();size=int(header[2]);data=batch.stdout.read(size)
 if len(data)!=size or batch.stdout.read(1)!=b'\n':raise RuntimeError('git blob framing')
 root_bytes[name]=data
batch.stdin.close()
if batch.wait()!=0:raise RuntimeError('git blob reader exit')
manifest={'root-head':head,'rax-parent':subprocess.check_output(['git','rev-parse','HEAD'],cwd=rax,text=True).strip(),'source-files':[],'patches':[],'environment-adapters':old['environment-adapters'],'root-policy':'Committed HEAD bytes; concurrent user worktree edits excluded.'}
bundle=share/'native-etw-provider-traits-current-overlay.zip'
with zipfile.ZipFile(bundle,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=1) as archive:
 for repo,directory in [('root',root),('rax',rax)]:
  paths=list(root_bytes) if repo=='root' else subprocess.check_output(['git','ls-files','-z'],cwd=directory).decode().split('\0')
  if repo=='rax':paths+=subprocess.check_output(['git','ls-files','--others','--exclude-standard','-z'],cwd=directory).decode().split('\0')
  for name in sorted(set(filter(None,paths))):
   if name.startswith('vendor/') and repo=='root':continue
   if repo=='rax' and name=='.cargo/config.toml':continue
   path=directory/name
   if not path.is_file():continue
   data=root_bytes[name] if repo=='root' else path.read_bytes()
   row={'repo':repo,'path':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()};manifest['source-files'].append(row)
   if previous.get((repo,name))!=row['sha256']:
    patch=dict(row,transfer='payload/'+repo+'/'+name);manifest['patches'].append(patch);archive.writestr(patch['transfer'],data)
 archive.writestr('manifest.json',json.dumps(manifest,indent=2)+'\n')
(base/'native-etw-provider-traits-current-overlay-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
loader=(prior/'native-wnf-process-event-current-overlay-bundle.py').read_text().replace('native-wnf-process-event','native-etw-provider-traits')
for destination in [base,share]:
 (destination/'native-etw-provider-traits-current-overlay-bundle.py').write_text(loader)
 (destination/'native-etw-provider-traits-current-overlay.ps1').write_text("$ErrorActionPreference='Stop'\n& 'C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe' 'C:\\Mac\\Home\\Downloads\\assist-native-allocate-ex-transfer-20261010-2154\\native-etw-provider-traits-current-overlay-bundle.py'\nexit $LASTEXITCODE\n")
print('sources',len(manifest['source-files']),'patches',len(manifest['patches']),'zip bytes',bundle.stat().st_size,flush=True)
