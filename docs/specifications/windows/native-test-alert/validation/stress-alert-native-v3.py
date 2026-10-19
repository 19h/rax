from pathlib import Path
import hashlib,json,re,runpy,shutil,tempfile
P=Path(__file__).parent;R=Path('/Users/int/.assist-alert3-20261019');D=R/'docs/specifications/windows/native-test-alert'
def load(p):
 raw=p.read_bytes();return json.loads(raw.decode('utf-16')if raw[:2]in[b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
def change_text(root,name,old,new):
 p=root/'native'/name;text=p.read_text();changed=text.replace(old,new,1);assert text!=changed;p.write_bytes(changed.encode())
def change_json(root,name,fn):
 p=root/'native'/name;value=load(p);fn(value);p.write_bytes((json.dumps(value)+'\n').encode())
def corrupt_machine(root):
 for part in ['before','after']:change_json(root,'native-test-alert-v1-arm64-'+part+'.json',lambda x:x.update(machine=0x8664))
def bind(root):
 p=root/'native/manifest.json';d=load(p)
 for row in d['files']:
  raw=(root/'native'/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
 p.write_bytes((json.dumps(d)+'\n').encode())
controls=[
 ('zero-argument-leaf-ret',lambda r:change_json(r,'alert-service-leaf-observation-b.json',lambda rows:rows[1]['exports']['NtTestAlert'].update({'bytes64-hex':rows[1]['exports']['NtTestAlert']['bytes64-hex'][:24]+'c2'+rows[1]['exports']['NtTestAlert']['bytes64-hex'][26:]}))),
 ('transition-slot-drift',lambda r:change_json(r,'alert-service-leaf-observation-b.json',lambda rows:rows[1]['exports']['NtTestAlert']['transition-thunk'].update({'bytes6-hex':'ff2518103c4b'}))),
 ('projection-drift',lambda r:(r/'modeled-cases.json').write_text((r/'modeled-cases.json').read_text().replace('00000101','00000000',1))),
 ('normalized-raw-drift',lambda r:(r/'cases.json').write_text((r/'cases.json').read_text().replace('00000101','00000000',1))),
 ('alert-priority',lambda r:change_text(r,'native-test-alert-v1-arm64.log','case=1 call=0 status=00000101','case=1 call=0 status=00000000')),
 ('fifo',lambda r:change_text(r,'native-test-alert-v1-arm64.log','event case=4 index=0 phase=4 kind=1 a=1111','event case=4 index=0 phase=4 kind=1 a=2222')),
 ('nested-phase',lambda r:change_text(r,'native-test-alert-v1-arm64.log','event case=6 index=1 phase=6','event case=6 index=1 phase=4')),
 ('callback-last-error',lambda r:change_text(r,'native-test-alert-v1-arm64.log','case=12 call=0 status=00000000 last=0000ABCD','case=12 call=0 status=00000000 last=0000C0DE')),
 ('context-restore',lambda r:change_text(r,'native-test-alert-v1-arm64.log','nonvolatile=1','nonvolatile=0')),
 ('native-callback-args',lambda r:change_text(r,'native-test-alert-v1-arm64.log','a=5555 b=5556 c=5557','a=5555 b=5557 c=5557')),
 ('callback-thread',lambda r:change_text(r,'native-test-alert-v1-arm64.log',next(x for x in (r/'native/native-test-alert-v1-arm64.log').read_text().splitlines()if x.startswith('event case=3 ')),re.sub(r'tid=[0-9A-F]+','tid=00000000',next(x for x in (r/'native/native-test-alert-v1-arm64.log').read_text().splitlines()if x.startswith('event case=3 '))))),
 ('profile-width',lambda r:change_text(r,'native-test-alert-v1-arm64.log','pointer-bytes=8','pointer-bytes=4')),
 ('machine-pair',corrupt_machine),
 ('source-drift',lambda r:(r/'native/native-test-alert-v1.cpp').write_bytes((r/'native/native-test-alert-v1.cpp').read_bytes()+b'\n')),
]
runpy.run_path(str(D/'check_native.py'))['check']();results=[]
with tempfile.TemporaryDirectory(prefix='alert-native-controls-',dir=R/'target')as temp:
 for name,mutate in controls:
  root=Path(temp)/name;shutil.copytree(D,root);mutate(root);bind(root)
  try:runpy.run_path(str(root/'check_native.py'))['check']()
  except (AssertionError,ValueError,KeyError)as exc:results.append({'name':name,'rejected':True,'error-type':type(exc).__name__})
  else:raise AssertionError('Accepted native corruption '+name)
  shutil.rmtree(root);print('Rejected',name,flush=True)
(P/'alert-native-negative-controls-v3.json').write_text(json.dumps({'controls':results,'all-rejected':True,'scope':'Each mutated native file is rebound into its outer manifest; rejection depends on independent semantic/ABI/source checks.'},indent=2)+'\n')
