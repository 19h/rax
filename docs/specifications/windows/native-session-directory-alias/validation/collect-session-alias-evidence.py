"""Publish source identities and filtered consumer evidence, preserving originals."""
from pathlib import Path
import hashlib,json,shutil

CACHE=Path(__file__).parent
SHARE=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
ROOT=Path('/Users/int/.assist-session-alias-20261020')
DOCS=ROOT/'docs/specifications/windows/native-session-directory-alias'
VALIDATION=DOCS/'validation'
VALIDATION.mkdir(parents=True,exist_ok=True)
projections=[]
def load(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def copy(source,target):
 raw=source.read_bytes();public=raw
 if source.suffix in ['.log','.jsonl']:
  kept=[];removed=[]
  for line in raw.splitlines(keepends=True):
   try:record=json.loads(line)
   except(json.JSONDecodeError,UnicodeDecodeError):record={}
   if record.get('reason')=='compiler-message'and'assist-rs'in record.get('package_id',''):removed.append({'bytes':len(line),'sha256':hashlib.sha256(line).hexdigest()})
   else:kept.append(line)
  if removed:
   public=b''.join(kept);projections.append({'path':target.relative_to(ROOT).as_posix(),'kind':'compiler-diagnostic-projection','original':{'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()},'published':{'bytes':len(public),'sha256':hashlib.sha256(public).hexdigest()},'withheld-records':removed,'preserved-semantics':'Complete private diagnostic records omitted before first publication; every remaining compiler/artifact/process/test byte is preserved. Complete original stream remains private.'})
 if source.suffix in ['.log','.jsonl']:
  assert not (b'claude_process' in public and b'warning: unused import:' in public),str(source)
 if target.exists():assert target.read_bytes()==public,str(target)
 else:target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(public)
def receipt(base,name,destination):
 value=load(base/name);assert value['state']=='finished'and value['exit-code']==0,name;copy(base/name,VALIDATION/destination)
 for row in value['steps']:
  path=base/row['log'];raw=path.read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']);copy(path,VALIDATION/path.name)
 return value
native=load(SHARE/'native-session-alias-v1-launch.json');assert native['state']=='finished'and native['exit-code']==0
copy(SHARE/'native-session-alias-v1.cpp',DOCS/'native-session-alias-v1.cpp');copy(SHARE/'run-native-session-alias-v1.py',VALIDATION/'run-native-session-alias-v1.py');copy(SHARE/'native-session-alias-v1-launch.json',VALIDATION/'native-session-alias-v1-launch.json')
for row in native['steps']:
 raw=(SHARE/row['log']).read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']);copy(SHARE/row['log'],VALIDATION/row['log'])
copy(SHARE/'namespace-kernel-profile.json',VALIDATION/'kernel-profile.json')
for host,base,gates,owner,before,after in [
 ('macos',CACHE,'session-alias-macos-epoch1-gates.json','macos-session-alias-epoch1-owning-gates.json','macos-session-alias-epoch1-c-pre-cpp-archive-identities.json','macos-session-alias-epoch1-c-final-owning-archive-identities.json'),
 ('linux',CACHE,'session-alias-linux-epoch1-gates.json','linux-session-alias-epoch1-owning-gates.json','linux-session-alias-epoch1-pre-cpp-archive-identities.json','linux-session-alias-epoch1-final-owning-archive-identities.json'),
 ('windows',SHARE,'session-alias-native-epoch1-gates.json','session-alias-native-owning-epoch1.json','native-session-alias-epoch1-pre-cpp-archives.json','native-session-alias-epoch1-final-owning-archive-identities.json')]:
 receipt(base,gates,'gates-'+host+'.json');receipt(base,owner,'owning-'+host+'.json');copy(base/before,VALIDATION/('archives-'+host+'-before.json'));copy(base/after,VALIDATION/('archives-'+host+'-after.json'))
receipt(SHARE,'session-alias-native-ordinary-epoch2.json','ordinary.json');receipt(SHARE,'session-alias-owning-capi-epoch2.json','capi.json')
copy(SHARE/'session-alias-retained-owning-epoch1.json',VALIDATION/'retained-owning.json')
files=['session-alias-source-epoch1.json','session-alias-source-epoch1-archive.json','session-alias-owning-source-hashes-epoch1.json','session-alias-ownership.json','session-alias-native-source.json','session-alias-region-parent-integration.json','session-alias-projection-parent-integration.json','session-alias-baseline.json','session-alias-baseline.log','session-alias-baseline-epoch2.json','session-alias-baseline-epoch2.log','session-alias-focus-epoch1.json','session-alias-focus-epoch1.log','session-alias-windows-cross-epoch1.json','session-alias-windows-cross-epoch1-aarch64.log','session-alias-windows-cross-xwin-epoch2.json','session-alias-windows-cross-xwin-epoch2-aarch64.log','session-alias-windows-cross-xwin-epoch2-x86_64.log','session-alias-macos-owning-link.json','macos-session-alias-epoch1-linkage.json','observer-crate-name-correction.json','session-alias-ordinary-owner-epoch2.rs','session-alias-observer-source-epoch2.json','session-alias-owning-capi.cpp','session-alias-owning-capi-source.json','freeze-session-alias-source.py','prepare-session-alias-owning-epoch1.py','run-session-alias-unix-epoch1.py','run-session-alias-native-epoch1.py','run-session-alias-owning-epoch1.py','run-session-alias-native-owning-epoch1.py','run-session-alias-native-ordinary-epoch2.py','run-session-alias-owning-capi-epoch2.py','collect-session-alias-archives-epoch1.py','session-alias-owned-cpp-build-epoch1.py','owning-linux-cpp-build-session-alias-epoch1.py','run-session-alias-focus.py','check-session-alias-windows-cross.py','check-session-alias-windows-xwin.py','collect-session-alias-evidence.py']
for name in files:copy(CACHE/name,VALIDATION/name)
for path in(CACHE/'baseline-source').rglob('*'):
 if path.is_file():copy(path,VALIDATION/path.relative_to(CACHE))
for name in ['session-alias-native-ordinary-epoch1.json','session-alias-native-ordinary-epoch1-build.log','session-alias-owning-capi-epoch1.json']:copy(SHARE/name,VALIDATION/('rejected-'+name))
for helper in load(CACHE/'session-alias-owning-source-hashes-epoch1.json')['helpers']:
 path=CACHE/helper['path']
 if not path.exists():path=SHARE/helper['path']
 copy(path,VALIDATION/helper['path'])
manifest_path=ROOT/'docs/development/evidence-projection/manifest.json';manifest=load(manifest_path);assert len(projections)==2
for row in projections:
 old=next((entry for entry in manifest['files']if entry['path']==row['path']),None)
 if old:assert old==row
 else:manifest['files'].append(row)
manifest['files'].sort(key=lambda row:row['path']);manifest['projected-streams']=sum(row['kind'].startswith('compiler-diagnostic-')for row in manifest['files']);manifest_path.write_text(json.dumps(manifest,indent=2)+'\n')
(VALIDATION/'public-stream-projections.json').write_text(json.dumps({'files':projections,'scope':'Compiler diagnostics removed before first publication. Complete raw producer receipts retain their original identities; exact public projections are versioned in the shared adapter table.'},indent=2)+'\n')
value={'source-files':25085,'source-bytes':1835122521,'source-epoch':'original-frozen-epoch1-with-explicit-public-data-projections','owning-source-inputs':3826,'library-gates':{host:{'passed':passed,'failed':5 if host=='windows' else 0,'ignored':2}for host,passed in [('macos',8102),('linux',8096),('windows',7564)]},'native-profiles':['arm64','x64','x86','x86-laa'],'native-kernel':'10.0.29683 ARM64','compiled-portable-alias-tests':3,'compiled-native-acquisition-tests':1,'ci-session-alias-target':'unknown','ordinary-smoke-exit':0,'whoami-frontier':'NtQueryAttributesFile0x3d','retained-assist-abi':184,'compared-current-root-abi':272,'full-process-goal-complete':False,'whole-current-root-products-validated':False,'independent-x64-kernel-validated':False}
(VALIDATION/'summary.json').write_text(json.dumps(value,indent=2)+'\n');(DOCS/'.gitattributes').write_text('* -text whitespace=cr-at-eol\n');print('collected',len(list(DOCS.rglob('*'))),'entries; diagnostics filtered',len(projections),flush=True)
