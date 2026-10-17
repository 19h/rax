from pathlib import Path
import hashlib,json,shutil,subprocess
B=Path(__file__).resolve().parent;R=B/'rax-source'
initial=json.loads((B/'initial-overlay.json').read_text())['files']
for row in initial:
 p=R/row['path'];raw=p.read_bytes()
 if hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('initial input changed '+row['path'])
for row in initial:
 p=B/'source-epochs/initial'/row['path'];p.parent.mkdir(parents=True,exist_ok=True);p.write_text(row['text'])
p=R/'src/user/windows/process/sched/tests/services_tests.rs';s=p.read_text()
if s.count('mod wnf_query_tests;')!=1:raise RuntimeError('test registration ambiguity')
p.write_text(s.replace('mod wnf_query_tests;','#[path = "services_tests/wnf_query_tests.rs"]\nmod wnf_query_tests;'))
p=R/'src/user/windows/wnf/query_tests.rs';s=p.read_text();s='use crate::user::windows::nt::status::*;\nuse std::{io,sync::Arc};\n'+s
s+='''
#[cfg(windows)]
#[test]
fn native_query_snapshot_refuses_an_owned_impersonation_scope_and_reverts() {
    #[link(name="advapi32")]
    unsafe extern "system" {
        fn ImpersonateSelf(level: u32) -> i32;
        fn RevertToSelf() -> i32;
    }
    assert!(super::installed_query::ensure_primary_thread().is_ok());
    struct Revert(bool);
    impl Drop for Revert {
        fn drop(&mut self) { if self.0 { unsafe { RevertToSelf(); } } }
    }
    // The test owns this test-runner thread; never alter another thread token.
    assert_ne!(unsafe { ImpersonateSelf(2) },0);
    let mut guard=Revert(true);
    let error=super::snapshot().unwrap_err();
    assert_eq!(error.kind(),io::ErrorKind::Unsupported);
    assert!(error.to_string().contains("impersonation"));
    assert_ne!(unsafe { RevertToSelf() },0);guard.0=false;
    assert!(super::installed_query::ensure_primary_thread().is_ok());
}
''';p.write_text(s)
p=R/'src/user/windows/wnf/installed_query.rs';s=p.read_text().replace('    fn GetCurrentProcessId() -> u32;','    fn GetCurrentProcessId() -> u32;\n    fn GetCurrentThread() -> *mut c_void;\n    fn CloseHandle(handle: *mut c_void) -> i32;')
pos=s.index('type Query =')
s=s[:pos]+'''#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenThreadToken(thread: *mut c_void, access: u32, open_as_self: i32, token: *mut *mut c_void) -> i32;
}

pub(super) fn ensure_primary_thread() -> io::Result<()> {
    let mut token=std::ptr::null_mut();
    // SAFETY: fixed pseudo current thread, TOKEN_QUERY only, exclusive output.
    // An existing token is closed before refusing effective-token ambiguity.
    if unsafe { OpenThreadToken(GetCurrentThread(),8,1,&mut token) } != 0 {
        if unsafe { CloseHandle(token) } == 0 { return Err(io::Error::last_os_error()); }
        return Err(io::Error::new(io::ErrorKind::Unsupported,
            "native WNF query selection refuses host thread impersonation"));
    }
    let error=io::Error::last_os_error();
    if error.raw_os_error()!=Some(1008) { return Err(error); }
    Ok(())
}

''' + s[pos:]
s=s.replace('pub(super) fn snapshot(definitions: Vec<Definition>) -> io::Result<Queries> {','pub(super) fn snapshot(definitions: Vec<Definition>) -> io::Result<Queries> {\n    ensure_primary_thread()?;').replace('    capture(definitions, session, |definition, bytes| {','    let selected = capture(definitions, session, |definition, bytes| {')
if not s.endswith('    })\n}\n'):raise RuntimeError('capture terminator changed')
s=s[:-len('    })\n}\n')]+'''    })?;
    ensure_primary_thread()?;
    Ok(selected)
}
''';p.write_text(s)
changed=['src/user/windows/process/sched/tests/services_tests.rs','src/user/windows/wnf/query_tests.rs','src/user/windows/wnf/installed_query.rs']
subprocess.run(['rustup','run','1.95.0','rustfmt','--edition','2024','--config','skip_children=true',*changed],cwd=R,check=True)
rows=[]
for row in initial:
 raw=(R/row['path']).read_bytes();rows.append({'path':row['path'],'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'previous-sha256':row['sha256'],'text':raw.decode()})
actual=[r['path'] for r in rows if r['sha256']!=r['previous-sha256']]
if sorted(actual)!=sorted(changed):raise RuntimeError('unexpected correction surface')
(B/'epoch2-overlay.json').write_text(json.dumps({'base':'7279261c3ab518e0d0b027817cabf776e9700eb8','files':[r for r in rows if r['path'] in changed]},indent=2)+'\n')
(B/'epoch2-source-inputs.json').write_text(json.dumps({'files':[{k:v for k,v in row.items() if k!='text'} for row in rows]},indent=2)+'\n')
manifest=json.loads((B/'initial-owning-source-hashes.json').read_text())
for row in manifest['sources']:
 raw=(R/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
(B/'epoch2-owning-source-hashes.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Preserved initial eleven inputs; corrected three files;325 selected source identities')
