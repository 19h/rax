from pathlib import Path
import datetime
import hashlib
import json
import shutil
import subprocess

CACHE = Path(__file__).parent
ROOT = Path('/Users/int/.assist-region-20261020')
DOCS = ROOT / 'docs/specifications/windows/native-region-information'
VALIDATION = DOCS / 'validation'
SHARE = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
NS = Path('/Users/int/Library/Caches/assist-process-emulation/native-namespace-20261020')

def load(path):
    return json.loads(path.read_text())

def write(name, value):
    (VALIDATION / name).write_text(json.dumps(value, indent=2) + '\n')

shutil.copy2(SHARE / 'namespace-kernel-profile.json', VALIDATION / 'kernel-profile.json')
shutil.copy2(SHARE / 'region-retained-owning-epoch1.json', VALIDATION / 'retained-owning.json')
shutil.copy2(SHARE / 'retain-region-artifacts-epoch1.py', VALIDATION / 'retain-region-artifacts-epoch1.py')
assumptions = load(CACHE / 'region-current-assumptions.json')
assumptions['validation-state'] = 'source epoch1 complete three-host gates; exact known Windows failures retained; actual owning-core and CAPI frontier advanced'
for row in assumptions['assumptions']:
    row['status'] = 'confirmed' if row['id'] in ['MRI-A2', 'MRI-A3', 'MRI-A5'] else 'retained'
    row['revisit-evidence'] = {
        'MRI-A1': 'Native15successfulversions, full19region/6copy focuses on three hosts, unfiltered library suites and actual loader execution agree for checked lifecycles; additional kernels remain unverified.',
        'MRI-A2': 'Nativev13/v14, guestreservation lifecycle, replayencoding and installedNtAllocateVirtualMemoryEx leaf pass.',
        'MRI-A3': 'Kernel profile/build29683/architecture12 plus stable sixDLL identities; no independent x64kernel claim.',
        'MRI-A4': 'Actualx86/x86compat/ARM64 instruction tests, warm/cold/raw/concurrent checks and all3 complete library binaries exercise acquisition; mappingchanges retain existing stopped-CPU contract.',
        'MRI-A5': 'Before/after archive/member/source identities, 7macCTests/6linkcommands,5CppLinux/Windows, and actualcore/carrier program runs; currentrootABI270 remains a distinct product boundary.',
    }[row['id']]
write('assumptions-final.json', assumptions)
base = 'dbcea8afa4292b4f5afa620963fe6f81e9301cae'
old = subprocess.check_output(['git', 'show', base + ':src/user/windows/memory.rs'], cwd=ROOT, text=True)
new = (ROOT / 'src/user/windows/memory/region.rs').read_text()
for begin, end in [('/// `MEMORY_BASIC_INFORMATION` in host form.', '/// Why a virtual-memory operation'),
                   ('    /// `VirtualQuery` of `addr`:', '    /// Writes `data` at `addr`')]:
    part = old[old.index(begin):old.index(end)]
    assert part.strip() in new, begin
frozen = load(VALIDATION / 'region-source-epoch1.json')
for row in frozen['files']:
    path = ROOT / row['path']
    raw = path.readlink().as_posix().encode() if path.is_symlink() else path.read_bytes()
    assert (len(raw), hashlib.sha256(raw).hexdigest()) == (row['bytes'], row['sha256']), row['path']
owned = load(CACHE / 'region-ownership.json')['owned-source-paths']
assert len(owned) == 28
write('source-review.json', {
    'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'base': base,
    'owned-inputs': owned, 'frozen-inputs-verified': len(frozen['files']),
    'public-api': {'RegionInfo-and-query-moved-byte-identically': True, 'original-public-path-reexported': True,
                   'Mapping-Vma-Allocation-public-fields-unchanged': True, 'cabi': '1.14.0', 'dependency-default-changes': False},
    'reviewed-mechanics': [
        'Process query/limited rights, complete prefix layout and native/WoW ordering are shared ABI mechanics; caller preprobes WoW ReturnLength.',
        'Acquisition follows permission validation on warm/cold translation and raw shared-readonly checks; successful write probes can charge before later errors.',
        'PTE bits4/5 are disjoint from permission bits0..2, valid3, and frame12..63; updates preserve unrelated flags.',
        'Mapping lock and stoppedguest mappingchange contract preserved; concurrent acquisition is atomic and idempotent.',
        'VMA update preserves eligibility; map/unmap clears; remap snapshots eligibility before removal then shifts ranges and PTE state; discard drops resident charge.',
        'Private commitment uses committed state runs; pagefile initialCOW reservedgap charge does not populateframes.',
        'InitialPEbytes precede tracking; loader relocations follow; initialsharedRW sections differ from runtimeprotect with sameprotection.',
        'NUMA metadata is inserted onlyafter successfulnewreservation and removed at release; existingcommit preservesit.',
        'No hostkernel forwarding, publiclayout/default/dependency mutation, or platform conditionalimplementation split.'
    ],
    'change-surface-map': {
        'shared-address-space': 'affected; opt-in interval/PTE tracking; full3host library suites and instruction/lifecycle tests',
        'windows-personality': 'affected; query, allocation, image/data/pagefile/scheduler map and release ownership',
        'linux-darwin-personalities': 'sharedmechanics compile/run; do not enable Windows privatecopy metadata; existing unfiltered suites preserved',
        'CAPI-and-Assist-carrier': 'actual rebuilt archives, feature[], source3823, structuralwalk, C++ link/runs on3hosts',
        'plugin-ui-mesh-permissions-persistence-SDK-update': 'no owning API/schema/layout/default changed; currentproducts not claimedvalidated by retainedABI184 boundary',
        'packages': 'no new runtimefile/library/schema; existingembeddedarchive consumers rebuilt; currentrootpackages remain unvalidated',
    },
    'findings': [
        {'impact': 'high', 'evidence': 'ordinary.json/capi.json', 'finding': 'Installedwhoami stops at NtQueryAttributesFile0x3d after12successfulregionqueries.', 'blocks-this-semantic-group': False, 'blocks-whole-goal': True},
        {'impact': 'high', 'evidence': 'GitLabpipeline71518/job424894 contextualerror; priorprivate trace retainedoutsidepublicproof', 'finding': 'Currentnonzero-session BaseNamedObjects entry is SymbolicLink; actualtargetunknown. Separateverifiedaliaswork remains.', 'blocks-this-semantic-group': False, 'blocks-whole-goal': True},
        {'impact': 'medium', 'evidence': 'gates-windows.json exact-existing-failures', 'finding': 'FiveestablishedAArch64lowererfailures remain explicit; no additionalfailure accepted.', 'blocks-this-semantic-group': False},
        {'impact': 'medium', 'evidence': 'kernel-profile.json/native/launchreceipts', 'finding': 'Nativeoracle spansfourABIviews ofoneARM64kernel, not independentphysicalx64kernel.', 'blocks-this-semantic-group': False},
        {'impact': 'high', 'evidence': 'summary.json/owningreceipts', 'finding': 'RetainedAssistABI184 consumers do not establishcurrentrootABI270 GUI/plugin/packageexecution.', 'blocks-this-semantic-group': False, 'blocks-whole-goal': True},
    ],
    'rejected-iterations': ['nativev9 schedulerprecondition code3', 'focus2 missingMemtrait compileerror', 'sourcearchive missingtargetdirectory retriedwithoutsourcechange', 'observerepoch3labels cancelledbeforegueststart; correctedepoch1 completed'],
})
write('quality-gates.json', {'scope': 'Completed regioninformation/privatecopy/NUMA semantic group only; broaderprocessgoal remainsactive.', 'gates': {
 'QG1': 'pass: Five assumptions have basis/dependents/stress/falsification and final status; bounded kernel/consumer profiles remain explicit.',
 'QG2': 'pass: Queryclass3/7 semantics, allocationcopy lifecycle, NUMA and source/consumer matrix have direct executed evidence.',
 'QG3': 'pass: Bytes/pages/layout widths, commands/configuration and independent source/runtime/PE hashes retained; replay reconstructed from rawrecords.',
 'QG4': 'pass: Guard/probe/capture ordering, sharedPE, reservedCOW, samevaluewrites, concurrent and mapping lifetime cases tested; remainingfrontiers labeled.',
 'QG5': 'pass: Nativeproducer source/PE/module hashes and one29683kernel primary observations; no fabricated x64kernel/currentproduct equivalence.',
 'QG6': 'pass: High/medium adjacent findings bounded and recorded; no unrelatedpolicy or dependency expansion.',
 'QG7': 'pass: Isolatedowned28source/test/model inputs; exactproofpath staging required; sharedcheckout concurrent state preserved.',
 'QG8': 'pass: Compiled0pass2fail baseline, focus19/copy6, complete3hostlibrary binaries,183CAPI and selectedintegration, owningconsumer paths and actualprogram results.',
 'QG9': 'pass: Explicitmodules/testregistration/publicreexport, source25082/1835096144bytes and owning3823 match frozen manifests; documents distinguishfullgoal fromthisgroup.'
}})
shutil.copy2(Path(__file__), VALIDATION / Path(__file__).name)
rows = []
for path in sorted(DOCS.rglob('*')):
    if path.is_file() and path.name != 'evidence-manifest.json':
        raw = path.read_bytes()
        rows.append({'path': path.relative_to(DOCS).as_posix(), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
(DOCS / 'evidence-manifest.json').write_text(json.dumps({'files': rows}, indent=2) + '\n')
print('sealed', len(rows), 'files', flush=True)
