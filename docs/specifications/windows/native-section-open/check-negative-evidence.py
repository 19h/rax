from pathlib import Path
import contextlib,hashlib,importlib.util,io,json,shutil,sys,tempfile
ROOT=Path(__file__).parent
if sys.flags.optimize:raise ValueError('optimized verification forbidden')
spec=importlib.util.spec_from_file_location('section_check',ROOT/'check-evidence.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
def require(ok,why):
 if not ok:raise ValueError(why)
def identity(root,mutate):
 p=root/'native-section-producer-identities.json';data=json.loads(p.read_text());mutate(data);p.write_text(json.dumps(data))
def replace(root,name,old,new):
 p=root/name;s=p.read_text();require(old in s,'negative mutation target');p.write_text(s.replace(old,new,1))
 # Recompute the claimed stdout identity so semantic controls are exercised.
 identity(root,lambda d:[x['stdout-identity'].update(bytes=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest())for x in d['producers']if x['log']==name])
mutations=[
 ('host-machine',lambda p:identity(p,lambda d:d.update({'native-machine':0x8664}))),
 ('principal',lambda p:identity(p,lambda d:d.update({'principal':'unknown'}))),
 ('kernel',lambda p:identity(p,lambda d:d.update({'kernel':[10,0,1,2,'']}))),
 ('inspector-source',lambda p:identity(p,lambda d:d['inspector-identity'].update(sha256='0'*64))),
 ('native-library',lambda p:identity(p,lambda d:d['native-libraries'][0].update(sha256='0'*64))),
 ('producer-count',lambda p:identity(p,lambda d:d['producers'].pop())),
 ('producer-profile',lambda p:identity(p,lambda d:d['producers'][0].update(profile='x64'))),
 ('source-identity',lambda p:identity(p,lambda d:d['producers'][0]['source-identity'].update(sha256='0'*64))),
 ('source-byte-count',lambda p:identity(p,lambda d:d['producers'][0]['source-identity'].update(bytes=1))),
 ('executable-ABI',lambda p:identity(p,lambda d:d['producers'][1]['executable-identity'].update(machine=0x14C))),
 ('LAA',lambda p:identity(p,lambda d:d['producers'][1]['executable-identity'].update(characteristics=d['producers'][1]['executable-identity']['characteristics']^0x20))),
 ('fresh-LR',lambda p:replace(p,'native-section-open-fresh-v3-arm64.log','lr=7FF8DE3C613C','lr=7FF8DE3C6140')),
 ('fresh-arguments',lambda p:replace(p,'native-section-open-fresh-v3-arm64.log',',D,',',1,')),
 ('fresh-root-grant',lambda p:replace(p,'native-section-open-fresh-v3-arm64.log','1000000003000000','1000000001000000')),
 ('fresh-section-grant',lambda p:replace(p,'native-section-open-fresh-v3-arm64.log','100000000D000000','1000000001000000')),
 ('fresh-service',lambda p:replace(p,'native-section-open-fresh-v3-arm64.log','function=55','function=54')),
 ('fresh-completion',lambda p:replace(p,'native-section-open-fresh-v3-arm64.log','done=1','done=0')),
 ('probe-partition',lambda p:replace(p,'native-section-open-probe-v3-arm64.log','case=0 family=0','case=0 family=1')),
 ('probe-cleanup',lambda p:replace(p,'native-section-open-probe-v3-arm64.log','exact-section-and-input-handles=1','exact-section-and-input-handles=0')),
 ('probe-birth',lambda p:replace(p,'native-section-open-probe-v3-arm64.log','birth-count=0','birth-count=1')),
 ('probe-exception',lambda p:replace(p,'native-section-open-probe-v3-arm64.log','exception=00000000','exception=C0000005')),
 ('probe-query-denial',lambda p:replace(p,'native-section-open-probe-v3-x86.log',':C0000022:4294967288:',':C0000022:0:')),
 ('probe-image-metadata',lambda p:replace(p,'native-section-open-probe-v3-arm64.log',':01800000:',':01000000:')),
 ('probe-typed-publication',lambda p:replace(p,'native-section-open-probe-v3-arm64.log',':Section:',':Directory:')),
 ('duplicate-partition',lambda p:replace(p,'native-section-open-duplicate-v2-arm64.log','parent=0','parent=1')),
 ('duplicate-cleanup',lambda p:replace(p,'native-section-open-duplicate-v2-arm64.log','exact-section-handles=1','exact-section-handles=0')),
 ('duplicate-error',lambda p:replace(p,'native-section-open-duplicate-v2-arm64.log','error=195936478','error=0')),
]
report=[]
for name,mutation in mutations:
 with tempfile.TemporaryDirectory(prefix='section-negative-')as temporary:
  p=Path(temporary);shutil.copytree(ROOT,p,dirs_exist_ok=True);mutation(p);module.ROOT=p
  try:
   with contextlib.redirect_stdout(io.StringIO()):module.check()
  except (ValueError,KeyError,StopIteration)as error:report.append({'mutation':name,'rejection':str(error)})
  else:raise ValueError('accepted controlled falsification: '+name)
module.ROOT=ROOT;require(len(report)==len(mutations),'negative count');print(json.dumps({'rejected':len(report),'cases':report},indent=2))
