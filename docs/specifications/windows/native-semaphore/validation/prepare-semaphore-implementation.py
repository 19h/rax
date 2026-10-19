from pathlib import Path
import argparse
import hashlib
import json
import subprocess


O = Path(__file__).resolve().parent
R = O / 'semaphore-source'
DRAFT = O / 'drafts'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def prepare():
    files = {}
    prefix = 'src/user/windows/dll/'
    mutant_path = prefix + 'native/mutant.rs'
    mutant = (R / mutant_path).read_text()
    failure_start = mutant.index('enum Failure {')
    failure_end = mutant.index('fn grant(')
    common_types = mutant[failure_start:failure_end]
    common_types = common_types.replace('enum Failure', 'pub(super) enum Failure')
    common_types = common_types.replace('type Checked', 'pub(super) type Checked')
    for function in ('status', 'finish', 'finish_with'):
        suffix = '<' if function == 'status' else '('
        common_types = common_types.replace('fn ' + function + suffix, 'pub(super) fn ' + function + suffix)
    name_start = mutant.index('fn name(')
    name_end = mutant.index('fn create_object(')
    common_name = mutant[name_start:name_end]
    common_name = common_name.replace('fn name(', 'pub(super) fn name(', 1)
    common_name = common_name.replace('opening: bool)', 'opening: bool, kind: &str)')
    common_name = common_name.replace('"NT mutant name outside selected guest directory namespace"',
                                      'format!("NT {kind} name outside selected guest directory namespace")')
    files[prefix + 'native/named_object.rs'] = '''//! Captured object-manager names, attributes, output faults and publication status.
pub(super) mod attributes;
use super::query::{guard_result, write_guard_result};
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::{Mem, MemFault};
use crate::user::windows::nt::status::*;
use crate::user::windows::object_namespace::{LookupError, NativeName};
use crate::user::windows::objects::Object;

''' + common_types + common_name
    attributes_path = prefix + 'native/mutant/attributes.rs'
    attributes = (R / attributes_path).read_text()
    attributes = attributes.replace('pub(super)', 'pub(in super::super)')
    attributes = attributes.replace('attrs: &Attributes) -> Checked<()>',
                                    'attrs: &Attributes, kind: &str) -> Checked<()>')
    attributes = attributes.replace('"NT mutant explicit security descriptor"',
                                    'format!("NT {kind} explicit security descriptor")')
    attributes = attributes.replace('"NT mutant security quality of service"',
                                    'format!("NT {kind} security quality of service")')
    files[prefix + 'native/named_object/attributes.rs'] = attributes
    files[attributes_path] = None
    mutant = mutant[:name_start] + mutant[name_end:]
    mutant = mutant[:failure_start] + mutant[failure_end:]
    mutant = mutant.replace('mod attributes;\n', '')
    mutant = mutant.replace('use super::query::{guard_result, probe_write, write_guard_result};',
        'use super::query::probe_write;\nuse super::named_object::{self, attributes, Checked, Failure, finish, finish_with, status};')
    mutant = mutant.replace('use crate::user::windows::memory::{Mem, MemFault};',
                            'use crate::user::windows::memory::Mem;')
    mutant = mutant.replace('use crate::user::windows::object_namespace::{LookupError, NativeName};\n', '')
    mutant = mutant.replace('attributes::supported(c, &attrs)?', 'attributes::supported(c, &attrs, "mutant")?')
    mutant = mutant.replace('let name = name(c, &attrs, opening)?;',
                            'let name = named_object::name(c, &attrs, opening, "mutant")?;')
    files[mutant_path] = mutant
    files[prefix + 'native/semaphore.rs'] = (DRAFT / 'semaphore.rs').read_text()
    path = prefix + 'native.rs'
    native = (R / path).read_text().replace('mod mutant;\n', 'mod mutant;\nmod named_object;\n')
    native = native.replace('mod secure_connect;\n', 'mod secure_connect;\nmod semaphore;\n')
    table = '''// Appended after the native wait table: existing ordinals and trap RVAs stay stable.
pub(super) static SEMAPHORE_EXPORTS: &[Export] = &[
    Export::func("NtCreateSemaphore", Stdcall, &[Ptr, I32, Ptr, I32, I32], semaphore::create),
    Export::func("NtOpenSemaphore", Stdcall, &[Ptr, I32, Ptr], semaphore::open),
    Export::func("NtReleaseSemaphore", Stdcall, &[Ptr, I32, Ptr], semaphore::release),
    Export::func("NtQuerySemaphore", Stdcall, &[Ptr, I32, Ptr, I32, Ptr], semaphore::query),
];

'''
    native = native.replace('fn status_error(', table + 'fn status_error(', 1)
    files[path] = native
    path = prefix + 'mod.rs'
    registry = (R / path).read_text().replace('.chain(native::WAIT_EXPORTS)',
        '.chain(native::WAIT_EXPORTS)\n        .chain(native::SEMAPHORE_EXPORTS)', 1)
    registry = registry.replace('exports: &[native::EXPORTS, slist::NTDLL_EXPORTS, native::WAIT_EXPORTS],',
        'exports: &[native::EXPORTS, slist::NTDLL_EXPORTS, native::WAIT_EXPORTS, native::SEMAPHORE_EXPORTS],', 1)
    files[path] = registry
    path = prefix + 'threading/tests.rs'
    helper = (R / path).read_text().replace('.chain(super::super::native::EXPORTS.iter())',
        '.chain(super::super::native::EXPORTS.iter())\n        .chain(super::super::native::SEMAPHORE_EXPORTS.iter())', 1)
    files[path] = helper
    path = prefix + 'threading/native_wait_tests.rs'
    test = (R / path).read_text()
    test = test.replace('.filter(|export| export.name != "NtWaitForSingleObject")',
        '.filter(|export| !["NtWaitForSingleObject", "NtCreateSemaphore", "NtOpenSemaphore", "NtReleaseSemaphore", "NtQuerySemaphore"].contains(&export.name))', 1)
    test = test.replace('before.symbols.len() + 1', 'before.symbols.len() + 5', 1)
    test = test.replace('after.symbols.last().unwrap().0', 'after.symbols[before.symbols.len()].0', 1)
    test = test.replace('// Reconstruct the preceding DLL by removing only this group\'s addition.',
        '// Reconstruct the pre-wait DLL by removing the wait and the later semaphore additions.')
    files[path] = test
    mapping = []
    for path, text in files.items():
        old = R / path
        target = DRAFT / 'prepared' / path
        if text is not None:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
        mapping.append({'path': path, 'before-sha256': sha(old.read_bytes()) if old.exists() else None,
                        'draft': str(target) if text is not None else None})
    (O / 'semaphore-proposed-source-map.json').write_text(json.dumps(mapping, indent=2) + '\n')
    print('Prepared', len(mapping), 'source operations outside the baseline source tree.')


def apply():
    baseline = json.loads((O / 'semaphore-baseline-macos.json').read_text())
    assert baseline['state'] == 'finished' and baseline['exit-code'] == 101
    assert any('0 passed; 9 failed;' in result for result in baseline['results']), baseline
    mapping = json.loads((O / 'semaphore-proposed-source-map.json').read_text())
    for row in mapping:
        path = R / row['path']
        assert (sha(path.read_bytes()) if path.exists() else None) == row['before-sha256'], row['path']
    for row in mapping:
        path = R / row['path']
        if row['draft'] is None:
            path.unlink()
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(Path(row['draft']).read_bytes())
    owned = [row['path'] for row in mapping if row['draft'] is not None]
    subprocess.run(['rustfmt', '+1.95.0', '--edition', '2024', '--config', 'skip_children=true', *owned], cwd=R, check=True)
    print('Applied and formatted the complete handler and dispatch change after the failing baseline.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    apply() if args.apply else prepare()
