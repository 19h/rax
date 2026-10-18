from pathlib import Path
import json,hashlib,shutil,subprocess,datetime
B=Path(__file__).resolve().parent;R=B/'rax-source';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
def load(p):
 raw=p.read_bytes();return json.loads(raw.decode('utf-16')if raw[:2]in[b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
if (B/'alignment-fix-applied.json').exists():raise RuntimeError('alignment fix already applied')
for host in ['macos','linux']:
 if not(B/(host+'-scalar-epoch1-focus.json')).exists():raise RuntimeError('first focus not finished '+host)
state=load(S/'scalar-epoch1-launch.json')
if state['state']!='finished':raise RuntimeError('native first focus not finished')
E=B/'rust-epoch1';E.mkdir(exist_ok=True)
for h in ['macos','linux']:
 for suffix in ['log','json']:shutil.copyfile(B/(h+'-scalar-epoch1-focus.'+suffix),E/(h+'-scalar-epoch1-focus.'+suffix))
for name in ['scalar-epoch1-launch.json','scalar-epoch1-driver.log','native-scalar-epoch1-focus.log','native-scalar-epoch1-gates.json']:
 shutil.copyfile(S/name,E/name)
shutil.copyfile(B/'scalar-epoch1-source.json',E/'scalar-epoch1-source.json')
paths=['src/user/windows/wnf/installed/scalars.rs','src/user/windows/process/sched/tests/services_tests/token_scalar_tests.rs'];before=[]
expected={r['path']:r for r in load(B/'scalar-epoch1-source.json')['files']}
for name in paths:
 p=R/name;raw=p.read_bytes();row=expected[name]
 if (len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('frozen first source drift '+name)
 q=E/'source'/(name+'.txt');q.parent.mkdir(parents=True,exist_ok=True);q.write_bytes(raw);before.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
p=R/paths[0];s=p.read_text();s=s.replace('let mut output = [0xA5u8; 4];','// A u32 enforces the native four-byte alignment, even for byte queries.\n        let mut native_output = 0xA5A5_A5A5u32;');s=s.replace('output.as_mut_ptr().cast()', '(&mut native_output as *mut u32).cast()');s=s.replace('        } as u32;\n        let value =', '        } as u32;\n        let output = native_output.to_ne_bytes();\n        let value =',1);p.write_text(s)
p=R/paths[1];s=p.read_text();s=s.replace('let mut data = [0xA5u8; 4];','// Match the native caller alignment independently of stack layout.\n            let mut native_data = 0xA5A5_A5A5u32;');s=s.replace('data.as_mut_ptr().cast()', '(&mut native_data as *mut u32).cast()');s=s.replace('            } as u32;\n            let value =', '            } as u32;\n            let data = native_data.to_ne_bytes();\n            let value =',1);p.write_text(s)
subprocess.run(['rustup','run','1.95.0','rustfmt','--edition','2024','--config','skip_children=true',*[str(R/p)for p in paths]],check=True)
(B/'alignment-fix-applied.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'before':before,'after':[{'path':name,'bytes':(R/name).stat().st_size,'sha256':hashlib.sha256((R/name).read_bytes()).hexdigest()}for name in paths],'reason':'Use u32 storage to guarantee the measured native four-byte alignment; guest query semantics unchanged.'},indent=2)+'\n')
print('Applied explicit native buffer alignment after collecting first focus outcomes')
