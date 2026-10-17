from pathlib import Path
import hashlib,json,shutil,subprocess
B=Path(__file__).resolve().parent;R=B/'rax-source'
prior=json.loads((B/'epoch3-source-inputs.json').read_text())['files']
for row in prior:
 raw=(R/row['path']).read_bytes()
 if (len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('prior source changed '+row['path'])
 p=B/'source-epochs/epoch3'/row['path'];p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw)
archive=B/'epoch3-validation';archive.mkdir(exist_ok=True)
for pattern in ['macos-wnf-final-*','linux-wnf-final-*','macos-owning-*','linux-owning-*','*-final-owning-archive-identities.json']:
 for p in B.glob(pattern):
  if p.is_file():shutil.copyfile(p,archive/p.name)
shutil.copyfile(B/'final-owning-source-hashes.json',archive/'final-owning-source-hashes.json')
p=R/'src/user/windows/dll/native/wnf_query.rs';s=p.read_text().replace('return Flow::ret(STATUS_INVALID_PARAMETER.into());\n                }\n            }\n            2 =>','return Err(c.unsupported("WNF query targets an uncaptured session"));\n                }\n            }\n            2 =>')
s=s.replace('return Flow::ret(STATUS_OBJECT_NAME_NOT_FOUND.into());\n                }\n            }\n            3 =>','return Err(c.unsupported("WNF query targets an uncaptured user"));\n                }\n            }\n            3 =>')
if 'uncaptured session'not in s or 'uncaptured user'not in s:raise RuntimeError('scope replacement missed')
p.write_text(s)
p=R/'src/user/windows/process/sched/tests/services_tests/wnf_query_tests.rs';s=p.read_text()
marker='fn reset(p: &mut Proc, b: u64) {'
helper='''fn unavailable(p: &mut Proc, t: &mut Thread, args: &[u64], reason: &str) {
    let sp=t.cpu.sp();let count=p.objects.handle_count();let rng=p.rng;
    super::nls_tests::arguments(p,t,args);
    if p.arch!=WinArch::Arm64 {t.cpu.set_gpr(0,0x21);}
    let result=if p.arch==WinArch::X86 {super::super::super::super::services::wow64(p,t,0x1234_0000)}else{handle_stop(p,t,stop(p.arch,0x21))};
    assert!(matches!(&result,Outcome::Fail(m) if m.contains("NtQueryWnfStateData")&&m.contains(reason)),"{result:?}");
    t.cpu.set_sp(sp);assert!(t.frames.is_empty());assert_eq!(p.objects.handle_count(),count);assert_eq!(p.rng,rng);
}
'''
if marker not in s:raise RuntimeError('helper insertion missing')
s=s.replace(marker,helper+marker)
s=s.replace('fn native_wnf_query_replays_session_user_machine_scope_and_typed_denial_controls()', 'fn native_wnf_query_replays_selected_scopes_and_refuses_uncaptured_namespaces()')
old='''            assert_eq!(
                invoke(p, &mut thread, &args),
                row["status"].as_u64().unwrap() as u32,
                "{profile} scope control{i}/{row}"
            );'''
new='''            let role=row["explicit"].as_str().unwrap();
            let refused=if scope==1 && matches!(role,"user-sid"|"world-sid"|"pid") {Some("uncaptured session")}
                else if scope==2 && role=="world-sid" {Some("uncaptured user")}else{None};
            if let Some(reason)=refused {unavailable(p,&mut thread,&args,reason);}
            else {assert_eq!(invoke(p,&mut thread,&args),row["status"].as_u64().unwrap()as u32,"{profile} scope control{i}/{row}");}'''
if old not in s:raise RuntimeError('scope assertion not found')
s=s.replace(old,new)
# Append the two actually observed cross-namespace failures to the same test,
# preserving the independently recorded status rather than faking equivalence.
needle='''        }
    }
}
''';at=s.rfind(needle)
if at<0:raise RuntimeError('scope test close missing')
extra='''        }
        let cross:serde_json::Value=serde_json::from_str(include_str!("../../../../../../../docs/specifications/windows/native-wnf-query/controls/wnf-owned-cross-scope.json")).unwrap();
        for row in cross["rows"].as_array().unwrap().iter().filter(|r|r["op"]=="query"&&r["target"]=="other") {
            assert_eq!(row["status"].as_u64(),Some(u64::from(STATUS_OBJECT_NAME_NOT_FOUND)));
            reset(p,b);let scope=row["scope"].as_u64().unwrap();
            p.space.w64(b,0x41C6_4E6D_A3BC_0074^(0x31|(scope<<6))).unwrap();
            p.space.wr(b+64,&guid()).unwrap();
            if scope==1 {p.space.w32(b+128,1).unwrap();}else{p.space.wr(b+128,&hex("010100000000000513000000")).unwrap();}
            p.space.w32(b+768,64).unwrap();
            unavailable(p,&mut thread,&[b,b+64,b+128,b+256,b+512,b+768],if scope==1 {"uncaptured session"}else{"uncaptured user"});
            assert_eq!(p.space.u32(b+256).unwrap(),0xA5A5_A5A5);assert_eq!(p.space.u32(b+768).unwrap(),64);assert_eq!(p.space.bytes(b+512,64).unwrap(),vec![0xA5;64]);
        }
    }
}
'''
s=s[:at]+extra+s[at+len(needle):];p.write_text(s)
d=R/'docs/specifications/windows/native-wnf-query'
for name in ['probe-owned-cross-scope.py','owned-cross-scope-controller.json','wnf-owned-cross-scope.json']:
 shutil.copyfile(B/name,d/'controls'/name)
m=json.loads((d/'inputs.json').read_text())
for name in ['probe-owned-cross-scope.py','owned-cross-scope-controller.json','wnf-owned-cross-scope.json']:
 p=d/'controls'/name;raw=p.read_bytes();m['files'].append({'path':'controls/'+name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
m['files'].sort(key=lambda r:r['path']);(d/'inputs.json').write_text(json.dumps(m,indent=2)+'\n')
changed=['src/user/windows/dll/native/wnf_query.rs','src/user/windows/process/sched/tests/services_tests/wnf_query_tests.rs']
subprocess.run(['rustup','run','1.95.0','rustfmt','--edition','2024','--config','skip_children=true',*changed],cwd=R,check=True)
fixture='docs/specifications/windows/native-wnf-query/controls/wnf-owned-cross-scope.json'
rows=[]
for name in sorted([r['path']for r in prior]+[fixture]):
 raw=(R/name).read_bytes();old=next((r['sha256']for r in prior if r['path']==name),None);rows.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'previous-sha256':old,'text':raw.decode()})
(B/'epoch4-source-inputs.json').write_text(json.dumps({'files':[{k:v for k,v in r.items()if k!='text'}for r in rows]},indent=2)+'\n')
(B/'epoch4-overlay.json').write_text(json.dumps({'base':'7279261c3ab518e0d0b027817cabf776e9700eb8','files':[r for r in rows if r['sha256']!=r['previous-sha256']]},indent=2)+'\n')
m=json.loads((B/'epoch3-owning-source-hashes.json').read_text());names=sorted({r['path']for r in m['sources']}|{fixture});m['sources']=[]
for name in names:
 raw=(R/name).read_bytes();m['sources'].append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
m.update(epoch=4,scope='327 current inputs; uncaptured namespace queries explicitly unavailable')
(B/'epoch4-owning-source-hashes.json').write_text(json.dumps(m,indent=2)+'\n')
(B/'final-owning-source-hashes.json').write_text(json.dumps(m,indent=2)+'\n')
print('Frozen corrected scope admission:',len(rows),'inputs;',len(m['sources']),'selected inputs')
