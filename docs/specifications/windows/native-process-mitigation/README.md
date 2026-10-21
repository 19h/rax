# Process system-call-filter information

This increment implements `NtQueryInformationProcess` class 52,
`ProcessMitigationPolicy`, for selector 11, `ProcessSystemCallFilterPolicy`.
It reports the standalone guest's existing unfiltered service table, with
FilterId zero. It does not query or change host process mitigation policy.
There is no accepted Win32k filter creation attribute or mitigation setter in
the current guest configuration. Nonzero filter enforcement, other selectors
and queries of unmodeled peer processes remain explicit unsupported operations.

The shared process-query path checks four-byte output alignment for nonzero
lengths and probes a nonnull ReturnLength for a four-byte write. The new branch
then requires length exactly 8 B, captures only the input selector word,
validates the process object and query grant, and writes only the flags word at
checked `output + 4`. It preserves the selector, remaining caller bytes, guest
LastError, PRNG and handle table. ReturnLength is never written, including on
success and when it aliases either output word. Its probe can still fault.

| Handle role | Valid selector, length and writable destinations |
|---|---|
| Current-process pseudo-handle | success, flags zero |
| Real self handle with `PROCESS_QUERY_INFORMATION` (`0x400`) | success |
| Real self handle with `PROCESS_QUERY_LIMITED_INFORMATION` (`0x1000`) | success |
| Real self handle with zero or only `PROCESS_VM_WRITE` (`0x20`) | `STATUS_ACCESS_DENIED` |
| Thread pseudo-handle or another object type | `STATUS_OBJECT_TYPE_MISMATCH` |
| Invalid or null handle | `STATUS_INVALID_HANDLE` |
| A represented peer process with adequate rights | explicit unsupported result |

The native selector read precedes handle validation. An inaccessible selector
therefore wins over an invalid handle, but an inaccessible flags word can lose
to that invalid handle. ReturnLength validation precedes both. It consumes a
reached guard, including a read-only guard, without publishing a byte. A guarded
flags word on a different page also consumes its guard and reports
`STATUS_GUARD_PAGE_VIOLATION`, including `READONLY|GUARD`. The common process
query guard handler implements this behavior; using the direct-store guard
policy from other NT services would incorrectly leave that guard armed.

The fixed DWORD accesses and 8 B record require O(1) work and scratch. Two
output pages and up to two ReturnLength pages can be visited. No caller-length
allocation, host handle forwarding, host pointer or IPC is introduced. Existing
ProcessCookie and console-process branches retain their own semantics.

Microsoft documents the general
[process query interface](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntqueryinformationprocess)
and [mitigation selector enumeration](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-process_mitigation_policy).
Its [system-call-filter structure](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/ns-ntddk-_process_mitigation_system_call_filter_policy)
has a four-bit FilterId and 28 reserved bits, but the page marks the structure
unsupported. The class-52 wire layout and probe/store order used here are
independent native observations on build 10.0.29683, rather than a public API
guarantee for other kernels. The guest-default interpretation and its concrete
falsification probe are recorded in `assumptions-and-surface.json`.

## Native controls and regression evidence

| Accepted producer | Per execution view | Purpose |
|---|---|---|
| V1 | 448 calls | 32 selectors x seven lengths x optional ReturnLength |
| V2 | 384 calls | eight handles x 24 guard/alignment/alias cases x two lengths |
| V4 | 384 calls | V2 with selector and guarded flags in separate pages |

All three producers complete ARM64, AMD64, x86 and x86-LAA execution views of
one ARM64 Windows kernel. These are not independent physical AMD64-kernel
oracles. The accepted total is `(448 + 384 + 384) x 4 = 4,864` recorded queries,
including repeated inputs between versions. All sources, executables, PE
architectures and runtime module identities remain stable. Private allocations
and duplicated handles are balanced. Every recorded query returns an NTSTATUS;
none escapes as an unhandled producer exception.

Rejected V3 did not place case16's flags in the intended guarded page. Its
source and observations remain distinct from accepted V4. Other policy values
in V1 can reflect their compiled producer's process attributes. Semantic parity
is asserted for selector 11, with complete V2/V4 matrices; no equivalence claim
is made for unimplemented selectors.

The original 1,180 B shared regression fails against published parent
`243a84ae7` and remains an unchanged prefix of the final test. Four focused tests
cover successful publication and aliases, exact lengths, unsupported selectors,
peer/object handling, and the complete V2/V4 records. All 3,072 rows replay on
each of three guest ABIs, checking returned status, two memory windows, three
page protections, guest LastError, PRNG, committed memory, handles and absence
of guest SEH frames. Six semantic oracle corruptions are rejected after
repairing transitive hashes and changing all execution views consistently;
each negative has a fresh successful restoration.

The immutable final source set has 26,913 inputs and 14 overrides, SHA-256
`2b7b4998edb31bc7e279f055cfba099fd0d404cb6c37877be1fea6c0ae0808ec`.
Its overrides are six Rust files and eight compiled native-log fixtures.
Candidate1's accessor compile error, candidate2's initial Windows PDB link
failure, the first retry's existing-temp-directory failure and Linux's initial
container mount failure remain separate from successful validation. Cleanup
removes only declared task-owned obsolete build outputs; native images, source,
logs and final owning artifacts are retained. No product or warning policy is
changed to bypass a failure.

| Host, no default features | Full library | C API | Integration |
|---|---|---|---|
| macOS ARM64 | 8,157 pass; zero failures; two existing ignores | 183 pass | 544 pass |
| Linux AMD64 container on ARM64 host | 8,151 pass; zero failures; two existing ignores | 183 pass | 544 pass |
| Native Windows ARM64 | 7,623 pass; five existing lowerer failures; two existing ignores | 183 pass | four pass |

All-target checks on all three hosts, both Windows cross targets, canonical
Linux all-target/default-feature Clippy and whole-tree rustfmt pass. The five
native Windows failures are exactly the recorded parent ARM64-lowerer set.
Retained actual Assist consumers pass on all hosts: eight selected-carrier link
rules and seven CTests on macOS, plus five compiled/run C++ consumers on Linux
and Windows. Complete archive walks bind their core/carrier pairs to 3,886
selected source inputs. Both native and complete-gate proof scripts pass on
macOS, Linux and Windows. The final live audit rehashes three producer sources,
12 executables, the ten-DLL union, the unit image, actual owning core/carrier and
both callers, plus four separately retained byte copies. Six public compiler
projections preserve original identities while withholding private Assist
compiler diagnostics. Raw native CRLF/progress records remain unchanged.

Both actual owning ordinary routes now advance whoami to
`NtAlpcSendWaitReceivePort`. Rust terminal index 2,816 corresponds to 2,817
C API turns. Reason 5 with an exit-code field of zero denotes emulator failure,
rather than guest exit. Both smoke routes exit zero. The measured next request
uses port handle `0x68`, flags `0x20000`, an aliased send/receive buffer with an
88 B message, and a caller receive capacity of `0x3B8` B (952 B). Optional
attribute and timeout pointers are null. Further ALPC/CSR behavior belongs to
the next increment.

This query, its proof and the retained Assist ABI184 consumers do not establish
completion of the full same-host userland objective or whole current Assist
ABI274 GUI/package execution. RAX C ABI 1.14.0, current Assist Rust ABI minor
274, dependency pins, defaults, permissions and CI definitions remain unchanged.
The ALPC frontier and independent image-view unwind-registration gap remain
high-impact requirements. No changelog or release work is included.
