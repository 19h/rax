# Installed Error Message Instrument registry metadata

The installed Windows runtime now selects the fixed root
`\Registry\Machine\System\CurrentControlSet\Control\Error Message Instrument`
through the existing immutable subtree capture mechanism. Before this change,
both actual Assist process callers stopped at an unsupported `NtOpenKey` for
that root. Acquisition now distinguishes observed absence from a present tree
and from other acquisition errors. Guest names and access masks never become
host registry calls.

Only this root and its descendants become selected. Parents, neighboring roots,
prefix collisions, NUL suffixes, and a literal `/child` name outside this root
remain unselected. A present root uses the existing bounded capture of raw
values and descendants; absence is not hard-coded. Native handles close before
guest publication. C API 1.14.0, dependency versions, defaults, permissions,
schemas and public layouts remain unchanged.

## Primary evidence and semantics

The original native producer sources, compiler/run records, PE observations,
and source/image/runtime hashes are under [native](native). V1 records 288
opens, and V2 records 576 additional opens. Each producer runs ARM64, x64,
x86 WoW64 and x86 WoW64 with large-address awareness on one ARM64 Windows
kernel. These are four execution views, not independent physical x64 evidence.

All 864 observed opens return `STATUS_OBJECT_NAME_NOT_FOUND` (`0xC0000034`),
clear the failed output handle and preserve `LastError=0x12345678`. No handles
are opened and the before/after process handle counts agree. V1 spans four
case/trailing-separator spellings, two attributes masks and nine access masks.
V2 also observes missing descendants, repeated separators, dot components and
a literal slash suffix. An observed absent slash-suffixed sibling does not
authorize selecting it in the guest.

The implementation uses `RegOpenKeyExW` with fixed query/enumerate access during
runtime selection. `ERROR_FILE_NOT_FOUND` at the root establishes absence;
access denial, path errors, enumeration failures, or an enumerated child
disappearing fail acquisition. Present data requires two matching bounded
captures and stable metadata. Existing limits remain: 4096 aggregate values,
16 MiB aggregate value bytes, 1024 keys per selected tree, depth 32, 255 UTF-16
code units per component, and 1 MiB aggregate tree path bytes. Existing limit
ownership is unchanged by adding another fixed root.

For K keys, V values, P path bytes and B value bytes, capture has bounded
O(K + V + P + B) transfer work plus existing per-node sorting. The retained
metadata uses O(K + V + P + B) space. Guest key handles own immutable records
after the selection object drops. Each UTF-16 code unit occupies 2 bytes;
the ordinary failing request's 152-byte name contains 76 code units.

```text
fixed host root -> absent / stable bounded tree / acquisition error
                         |
                 immutable guest selection
                         |
guest NtOpenKey -> existing capture/lookup/access/handle logic
```

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| ERI-A1 | The observed root is genuinely absent on the validation machine, rather than permanently absent on all Windows installations | Two source-bound native producers, 864 opens | Validation expectation for this installation | Present synthetic tree and controlled read-only native subtree; duplicate selection, aggregate budget, child disappearance | A successful native root open must produce a captured tree; errors other than root not-found must propagate | Confirmed for this installation; no universal absence claim |
| ERI-A2 | Selection remains confined to the fixed root | New enum/path entry; common lookup implementation | No admission of unrelated host metadata | Case, trailing/repeated separators, parents, neighboring names, prefix collisions, literal slash and NUL suffix | An unrelated path changing from Unselected to Present/Missing disproves confinement | Confirmed by portable tests |
| ERI-A3 | Source and consumers correspond to the frozen implementation | 28816-input source ledger, Cargo-selected archives and owning input envelope | Attribution of observed behavior | Independent native baseline fails; exact source checks, complete archive walks and stable hashes | A mismatched source/selected carrier hash or stale artifact selection disproves attribution | Confirmed for recorded source and consumers |

## Change-surface map

| Plane | Effect / evidence |
| --- | --- |
| Windows registry model | One additional compile-time `TreeRoot`, fixed path and bounded immutable selection |
| Installed Windows adapter | Existing acquisition mechanism selects this root; native tests exercise observed presence/absence and controlled present acquisition |
| NT service dispatch and object lifetime | Existing implementation reused; native open observations replay on all three guest ABIs; raw data and descendant handles survive registry drop |
| Linux/macOS hosts | Shared registry semantics compile/run through portable tests; Windows acquisition stays behind its existing adapter |
| ISA, SMIR, optimizer, native lowerers, JIT, backend, devices | No implementation changes: registry selection and tests do not alter instruction execution |
| C ABI, Assist policy, UI, persistence, discovery, packaging | No contract/default/resource changes; actual retained Assist consumers rebuilt against the new core |
| Tests/docs | Three portable model tests, three native adapter tests, one four-view replay test, native baseline reproduction and provenance checkers |

## Validation record

| Host | Unfiltered library | C API | Windows integration | Actual retained Assist consumers |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 8172 passed, 0 failed, 2 existing ignored | 183 passed | 544 passed | Eight links, seven CTests passed |
| Linux x64 container | 8166 passed, 0 failed, 2 existing ignored | 183 passed | 544 passed | Five C++ consumers passed |
| Windows ARM64 | 7641 passed, 5 existing lowerer failures, 2 existing ignored | 183 passed | 4 passed; other binary has zero tests under native cfg | Five C++ consumers passed |

The Windows source-bound baseline has 0 passed / 1 failed, independently showing
that the published parent omits this root. Candidate focused tests pass: four
on macOS/Linux, seven on Windows. The replay test exercises all 288 V1 opens on
each of the three guest ABIs (864 replayed calls per host). V2 is additional
independent validation of the inherited missing-descendant rule; its new raw
records are not additional compiler inputs.

Both Windows MSVC cross checks, canonical Linux Clippy and whole-tree Rustfmt
1.95 pass. The initial macOS full suite exposed two Unix socket tests whose long
task temporary directory triggered path translation; unchanged source passes
the complete retry with a short temporary root. Native baseline linking first
exhausted disk space; it passed setup and produced the expected regression
failure after lossless compression of eight task-owned retained artifacts.
The reported physical reduction was 1679068796 bytes, with all logical byte lengths and
SHA-256 values unchanged. Original failed attempts remain separate evidence.

Current full suites also execute the actual installed macOS `true`/`printf`
programs through dyld's shared cache and three Linux dynamic-runtime cases,
including an installed `libm.so.6` preload. These assertions require guest exit
0, exact output and actual loader/cache admission with no separate sysroot;
they do not establish arbitrary-program completeness. Linux evidence uses an
x64 Linux container translated on an ARM64 host.

The previous published ALPC pair is RAX
`b40dc96e98355fdcb35292654a108ea73ab3842e` and Assist
`aff0310364d2d8169ab1feba3cd428b9dbc227ba`. Its Assist pipeline 72003 passed all
nine jobs. RAX's main CI retained five Windows x64 environment/profile failures
and a cancelled macOS core job; other completed workflow results are recorded
separately. No all-green RAX CI claim is made.

Both actual owning callers retain smoke exit status 0 and advance fourteen
additional turns past the new selection. Whoami stops at the next `NtOpenKey`:
`\Registry\Machine\Software\Policies\Microsoft\Windows\Display`.
The read-only diagnostic observes `DesiredAccess=0x20019`, null RootDirectory,
`Attributes=0x240`, and a 122-byte (61 UTF-16-code-unit) name. Rust terminal index
2832 corresponds to 2833 C API turns; C API reason 5 with exit-code field zero
is an emulator failure, not a guest exit. The source, runtime, program and
selected archive hashes match before/after both routes.

## Reproduction and bounded findings

```sh
python3 -B docs/specifications/windows/native-error-message-instrument/check_native_evidence.py --source .
```

`--live-native` additionally rehashes the original native inputs on Windows.
The source ledger's SHA-256 is bound explicitly before checking individual
inputs. Reconstruct the frozen compiler envelope from the public Git baseline
and the 25 individually hashed members in
`validation/eri-final-compiled-overrides-v1.tar.gz`; the development materialization
archive is not recursively republished. Evidence checker corruption tests alter
all execution views with repaired stage digests, then verify a fresh restored
positive. Raw native records preserve their original bytes, including CRLF.

| Impact | Finding | Task boundary |
| --- | --- | --- |
| High | Both actual owning callers now reach the next `NtOpenKey`, for `\Registry\Machine\Software\Policies\Microsoft\Windows\Display` | Full-userland goal remains active |
| High | Existing installed CSR success remains the explicit ARM64/build/session profile | This registry change does not generalize ALPC or USER behavior |
| Medium | Existing Windows ARM64 lowerer failures and Windows x64 CI environment/profile assertions require separate resolution | No failure is concealed or converted to a skip |
| Medium | Retained Assist ABI184 consumers differ from current ABI274 whole-root products | Source/runtime/package claims remain separate |
| Medium | Long temporary roots expose inherited cross-host Linux socket behavior on macOS | Original failing run retained; no adjacent socket edit in this group |

Final native/source and complete-gate checkers pass on macOS, Linux and Windows.
The native live check verifies eighteen source/image/runtime inputs; the final
native artifact audit additionally checks the ten-DLL union, unit image, actual
owning core/carrier/callers and four separate retained byte copies. Full and C API
steps are carried byte-for-byte from the initial receipt when only unfinished
integration/all-target/ordinary gates resume; the source ledger is identical.
The early audit rejection and failed copy/PDB/materialization attempts remain
explicitly classified. Six publication projections retain original identities
while omitting private Assist compiler diagnostics; all other captured bytes
remain unchanged.

Quality gates: no normative judgment is required; assumptions ERI-A1--ERI-A3
are reconciled; requested source/consumer/platform representations are covered;
byte/code-unit arithmetic and limits are reproducible; rejected attempts and
edge cases remain explicit; primary sources and raw identities are verified;
bounded findings stay separate; exact owned-path staging preserves concurrent
work. Full-userland acceptance remains open. The original shared Assist checkout
and unrelated host builds/configuration remain untouched.
