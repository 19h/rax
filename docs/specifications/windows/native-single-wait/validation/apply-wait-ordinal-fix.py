from pathlib import Path
import datetime,hashlib,json,subprocess

N=Path(__file__).resolve().parent;R=N/'wait-source'
record=json.loads((N/'wait-ordinal-baseline-result.json').read_text())
text=(N/'wait-ordinal-baseline.log').read_text()
assert record['exit-code']==101 and 'running 1 test' in text and '0 passed; 1 failed;' in text
assert 'existing ordinals or trap RVAs changed' in text
assert not (N/'wait-ordinal-fix-applied.json').exists()
plan=json.loads((N/'wait-owned-source-plan.json').read_text())
assert subprocess.check_output(['git','diff','--','src/user/windows/dll/mod.rs'],cwd=R)==b''
plan['paths'].append('src/user/windows/dll/mod.rs')
plan['acceptance'].insert(2,'Preserve every pre-existing synthetic NTDLL export ordinal and trap RVA; append the new table after existing families')
(N/'wait-owned-source-plan.json').write_text(json.dumps(plan,indent=2)+'\n')
p=R/'src/user/windows/dll/native.rs';s=p.read_text()
block='''    Export::func(
        "NtWaitForSingleObject",
        Stdcall,
        &[Ptr, I32, Ptr],
        wait::single,
    ),
'''
assert s.count(block)==1;s=s.replace(block,'')
start=s.index('pub(super) static EXPORTS:')
end=s.index('\n];',start)+len('\n];')
s=s[:end]+'''\n\n// Append this table after all existing NTDLL families to preserve their
// synthetic PE ordinals and trap addresses. Kernel lookup shares this table.
pub(super) static WAIT_EXPORTS: &[Export] = &[
'''+block+'];'+s[end:]
p.write_text(s)
p=R/'src/user/windows/dll/mod.rs';s=p.read_text()
old='native::EXPORTS.iter().find_map(|export| {'
assert s.count(old)==1;s=s.replace(old,'native::EXPORTS.iter().chain(native::WAIT_EXPORTS).find_map(|export| {')
old='exports: &[native::EXPORTS, slist::NTDLL_EXPORTS],'
assert s.count(old)==1;s=s.replace(old,'exports: &[native::EXPORTS, slist::NTDLL_EXPORTS, native::WAIT_EXPORTS],')
p.write_text(s)
subprocess.run(['rustfmt','+1.95.0','--edition','2024','--config','skip_children=true','src/user/windows/dll/native.rs','src/user/windows/dll/mod.rs'],cwd=R,check=True)
# Existing primary native exports remain byte-identical to the published parent.
def body(s):
 start=s.index('pub(super) static EXPORTS:');return s[start:s.index('\n];',start)+len('\n];')]
parent=subprocess.check_output(['git','show','889bc914e298cb58d8ab0b90c9181ef75083f97c:src/user/windows/dll/native.rs'],cwd=R).decode()
assert body(parent)==body((R/'src/user/windows/dll/native.rs').read_text())
(N/'wait-ordinal-fix-applied.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'baseline':'wait-ordinal-baseline-result.json','changed':['src/user/windows/dll/native.rs','src/user/windows/dll/mod.rs'],'primary-native-export-body-identical-to-parent':True,'scope':'Append the new wait table after all existing synthetic NTDLL tables and share it with installed native service lookup. No existing export signature or order changes.'},indent=2)+'\n')
print('Preserved parent native table and appended wait exports after existing NTDLL families')
