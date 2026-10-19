# Native semaphore operations

This group implements `NtCreateSemaphore`, `NtOpenSemaphore`,
`NtReleaseSemaphore`, and `NtQuerySemaphore` in the guest Windows object model.
Installed NTDLL service dispatch and synthetic NTDLL exports use the same
handlers. Native and Win32 callers share names, access grants, counts, waits,
and object lifetime. Existing synthetic export ordinals and trap RVAs remain
unchanged for x86, x86-64, and ARM64 guests.

Both ordinary ARM64 programs advance past semaphore initialization. They still
stop at `NtOpenKey` because the requested key is outside the selected runtime
snapshot. Neither program has a successful guest exit.

## Contract and measured boundaries

Counts are signed 32-bit integers. Creation requires
`0 <= InitialCount <= MaximumCount` and `MaximumCount > 0`. These checks also
apply when `OBJ_OPENIF` finds an existing semaphore. Win32 creation retains its
existing behavior of ignoring counts when it opens an existing name.

Release requires `SEMAPHORE_MODIFY_STATE` (`0x0002`), a positive increment,
and a resulting count no greater than the maximum. Signed overflow returns
`STATUS_SEMAPHORE_LIMIT_EXCEEDED` (`0xC0000047`) without changing the count.
Queries require `SEMAPHORE_QUERY_STATE` (`0x0001`). Waiting uses the existing
`SYNCHRONIZE` (`0x00100000`) contract.

`SemaphoreBasicInformation` occupies eight bytes: a four-byte current count
followed by a four-byte maximum count. The measured native query probes that
fixed eight-byte destination, aligned to four bytes, before validating the
declared length. The optional four-byte `ReturnLength` may be unaligned and is
written last, including when it aliases the information buffer. WoW64 rejects
unknown information classes before either destination probe.

Native 64-bit create/open probes the output HANDLE first. Semaphore count
validation precedes native object-attribute capture. WoW64 converts attributes
before the output probe and performs its final HANDLE copy after kernel
validation. Replays retain the different guard and read-only outcomes.

Mutants and semaphores now share counted-name capture, directory resolution,
and fault-to-status conversion. Mutant access, ownership, recursion, release,
and query policy remain in the mutant implementation. Eleven existing mutant
tests, including their native differential corpora, pass after this extraction.

Count arithmetic and count-field copies take O(1) time and additional space,
apart from existing object/handle lookup. Capturing a name of L UTF-16 code
units takes O(L) time and space. Object storage does not grow with the numeric
initial or maximum count.

Names remain counted UTF-16, separate from filesystem paths. The selected
guest namespace supports the captured directory roots and Local/Global links.
The existing single-principal guest policy explicitly rejects security
descriptors, QoS, permanent objects, and `ACCESS_SYSTEM_SECURITY`. This group
does not claim full NT object-security or cross-process namespace equivalence.
No guest semaphore operation forwards a guest handle to the host kernel.

## Native evidence

All twelve captures ran on one Windows ARM64 kernel, version `10.0.29683`.
ARM64, x64 compatibility, x86 WoW64, and x86 WoW64 with LAA are four ABI
profiles on that kernel. They are not four independently tested kernels.
Sources, executable metadata, installed provider hashes before/after execution,
per-case results, and handle accounting are retained in the three native
directories.

| Producer | Cases per profile | Profiles | Cases |
| --- | ---: | ---: | ---: |
| Faults, counts, access and output publication | 5,235 | 4 | 20,940 |
| Names, collisions, lifetime and Win32 interoperability | 224 | 4 | 896 |
| Query probe/class/length precedence | 4,608 | 4 | 18,432 |
| Total | 10,067 | 4 | 40,268 |

All captures exit zero and preserve their initial handle count. The Rust
replays contain 39,308 cases: 6,144 create, 768 open, 4,480 release and 27,916
query cases. Only opaque successful HANDLE values are normalized. Output
bytes, error statuses, retained counts and guard states remain exact.

The other 960 native cases provide the named-object and access-grant evidence
used by the semantic regressions. The named producer verifies that native and
Win32 handles identify the same object and observes counts `0, 2, 3, 2` across
their wait/release operations.

`check_native.py` independently checks count/access/fault precedence before
comparing replay projections. Seventeen deliberate evidence corruptions are
rejected. Native-record corruptions rebind the replay projections, so a simple
projection mismatch cannot conceal a missing semantic check.

## Source and platform validation

The tests-only baseline compiles against RAX `464e728b` and fails all nine new
tests. The initial compile-only assertion-conversion error and its exact source
are retained separately. After implementation, the nine semaphore tests,
eleven mutant tests, and sixteen single-wait tests pass on macOS.

Source epoch 1 contains 22,581 repository inputs totaling 1,566,622,159 bytes.
The full core gates below use this snapshot. All-target checks also pass.

| Host | Core passed | Failed | Ignored | C API passed | Windows integration |
| --- | ---: | ---: | ---: | ---: | --- |
| macOS ARM64 | 7,970 | 0 | 2 | 168 | 544 portable; native-memory target has 0 cases |
| Linux x86-64 container | 7,964 | 0 | 2 | 168 | 544 portable; native-memory target has 0 cases |
| Windows ARM64 | 7,410 | 5 existing | 2 | 168 | Portable target has 0 cases; 4 native-memory cases |

The five Windows failures are the exact pre-existing AArch64 lowerer set
checked by `check_validation.py`. No assertion is weakened and no new test is
ignored to accommodate them.

RAX master advanced during validation with four independent C API commits,
ending at `c717f9b8`. Their eleven paths are confined to `capi/`; the ABI advances
from 1.11 to 1.14. The emulator core, fixtures, Cargo manifest/lockfile and
other validated inputs remain byte-identical. Core sources/tests contain no
C API source references, and the root crate has no build script.

The owned semaphore files were copied onto that committed head in a separate
worktree. Source epoch 2 contains 22,587 inputs totaling 1,566,675,854 bytes.
The changed C API is tested without filtering on all three hosts: 183 tests
pass on each. C API all-target checks and owning consumers are rebuilt on this
integrated snapshot. The epoch-1 full core results are retained with the exact
input-parity evidence rather than attributed to changed C API code.

Owning consumers bind 506 selected affected inputs; this is not the complete
Cargo dependency closure. Full repository snapshots are recorded separately.
Actual Cargo core/carrier paths, feature sets and complete archive-member walks
are recorded before and after C++ validation. Linux and Windows helpers bind
the exact archive they link. macOS records actual Ninja linker commands.

The retained consumer harness uses Assist Rust ABI 184. Eight C++ inputs were
compared with root `8f49854b` (Rust ABI 264). macOS runs seven CTests plus
protected CLI/MCP artifact checks; Linux and Windows run five C++ consumers
each. These lanes do not validate the whole current-root GUI, plugin or package.

## Ordinary installed-library observations

The read-only observer links the actual owning Windows core archive. Its
SHA-256 is
`0bd770c8168f7ab4fde931024a620a7aefc882b2ae7f373a423b2bce0401be97`.
The core archive is byte-identical across the two source epochs; the integrated
C API carrier changes. Source, observer executable, program bytes, the selected
core and six installed runtime modules are bound before/after observations.
Diagnostic service numbers are read from installed ARM64 NTDLL instructions;
they do not change guest dispatch.

Both supplied PE images use virtual image path `C:\app\probe.exe` with native
libraries enabled. The initial 2,100,000 observer turns execute normally before
detailed tracing. An observer turn is a scheduler execution budget unit, not a
claim about physical host instructions or elapsed time.

| Program | Prior stop at NtOpenSemaphore | New NtOpenKey stop | Additional observer turns |
| --- | ---: | ---: | ---: |
| ARM64 smoke | 2,151,527 | 2,178,927 | 27,400 |
| Installed ARM64 whoami.exe | 2,153,876 | 2,181,212 | 27,336 |

Each observes `STATUS_OBJECT_NAME_NOT_FOUND` from the initial semaphore open,
then two successful native semaphore creations with real guest count state.
Controller exit zero is separate from guest exit; the remaining registry
snapshot error is retained verbatim.

## Assumptions, scope and quality checks

`final-assumptions.json` records the final assumption register, stress tests,
falsification probes and dependent results. `plan-and-ownership.json` records
the earlier planning state. The current bounded findings are:

| Impact | Finding | Blocks |
| --- | --- | --- |
| High | Both ordinary programs still stop at registry snapshot coverage. | Full process-emulation goal |
| Medium | Five Windows AArch64 lowerer failures predate this group. | Full green core suite; no semaphore regression |
| Medium | Full NT security and cross-process object semantics remain outside this group. | Claims of full NT object equivalence |
| Medium | Retained consumers do not establish whole current-root product validation. | Whole GUI/plugin/package claims |

Run the bound evidence verifier with:

```text
python3 -B docs/specifications/windows/native-semaphore/check_validation.py
```

It checks native evidence, complete source snapshots and integration parity,
actual test selections, exact Windows baseline failures, current owning
archives, ordinary-program identities, and the remaining frontier.

Primary API declarations are pinned to PHNT commit
`53fbbdc5b5d2b08761db1c7b26bfa8c820924356`
([ntexapi.h](https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntexapi.h)).
The documented Win32 count and access contracts are in
[CreateSemaphoreW](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createsemaphorew)
and [synchronization access rights](https://learn.microsoft.com/en-us/windows/win32/sync/synchronization-object-security-and-access-rights).
Private fault/copy ordering is native observation on the stated kernel.
