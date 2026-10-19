# Owned NtContinue alertable context replacement

`NtContinue(ContextRecord, TestAlert)` validates and applies the supplied
architecture context before testing guest alerts or delivering queued APCs.
`TestAlert` is an eight-bit `BOOLEAN`: a zero low byte leaves alert and queue
state pending. With a nonzero byte, an existing user alert is consumed first
and leaves the APC queue intact. Otherwise pending single-argument Win32 APCs
run against a scheduler snapshot of the replacement context. FIFO and callbacks
appended during delivery are preserved. Return from delivery restores the target
registers; no synthetic wait or NTSTATUS overwrites the supplied result register.
Callback writes to guest TEB memory remain observable.

Native service and HLE export entry share the context-replacement ownership
rule. A changed target at an abandoned frame's original stack height discards
that continuation before any new APC frame is created. The compiled regression
also covers the non-alertable native path, which previously retained the stale
frame. Cancellation preserves an active callback and its remaining queue;
forced thread exit releases the continuation, stack and TEB ownership.

The public Rust `Flow` enum adds `ResumeAlertable`. The opaque C ABI, feature
defaults and dependencies are unchanged. Guest continuation and alert checks do
not invoke host NtContinue or host alert operations. Native three-argument APC
queue and alert producers remain separate unsupported frontiers.

The alert decision costs O(1). Context validation/restoration has fixed ABI size:
912 bytes for ARM64, 1232 bytes for x64 and 716 bytes for x86 in the recorded
native probes. Delivering K callbacks costs O(K) scheduler dispatches plus guest
execution; queue storage is O(K), and nested continuation storage is O(D) for
nesting depth D. Frame pruning is O(D) in the number of abandoned frames.

## Native reference boundary

One Windows build 29683 ARM64 kernel was observed through ARM64, x64, x86 and
x86 large-address-aware producer profiles. The corrected probe completes 52
captured-context resumes, 104 post-resume NtTestAlert checks and 48 callbacks.
Each worker owns its context, queue and alert. Tests include zero and nonzero
BOOLEAN values, preexisting alerts, FIFO delivery, append during delivery and
LastError mutation. The x86 capture wrapper independently checks the captured
PC and SP against the intended worker continuation. The first prototype's x86
capture selected its caller frame; that aborted attempt is retained separately.

The pinned PHNT declaration and license are in `primary/`. Raw source, driver,
compiler logs, PE identities, executable before/after hashes and six unchanged
runtime DLL identities are in `native/`. `check_native.py` independently checks
those records and requires `cases.json` to match their exact replay projection.
The results do not establish behavior on other kernel builds or physical x64
hardware.

## Historical epoch1 validation

The tests-only baseline compiled and failed all three selected tests while the
old implementation explicitly rejected alertable NtContinue. Nine shared
regressions now pass on macOS, including the complete native replay, HLE/native
parity, low-byte handling, invalid-context state preservation, replaced APC
ownership and cancellation/forced exit. Thirteen native-record corruptions are
rejected after the relevant launch and envelope hashes are rebound.

Complete source-bound validation now records:

| Host | Library passed | Failed | Ignored | C API passed | Integration passed | All targets |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| macOS | 8079 | 0 | 2 | 183 | 544 | pass |
| Linux | 8073 | 0 | 2 | 183 | 544 | pass |
| Windows | 7540 | 5 established lowerer failures | 2 | 183 | 4 | pass |

Nine shared and ten native Windows focused tests pass. The Windows selection
executes actual installed native/WoW64 NtContinue leaves and real CPU callbacks.
The compilation graph contains 25,069 files / 1,834,242,339 bytes. The switch from
the original unpublished parent to published alert commit 5d475727 changes only
prior derived proof, outside every compiled input; all 12 owned files and the
entire frozen source graph remain byte-identical.

Retained Assist ABI 184 validation passes seven macOS CTests and five C++
consumers each on Linux and Windows, with 3,811 inputs, actual Cargo archives,
full member walks, stable before/after identities and six macOS linker commands.
Whole current-root ABI 264 GUI/plugin/package behavior remains separate.

Actual owning Windows core: 508,624,474 bytes, SHA-256
`0654968feb60b5a26d6a6c971c33c569a06e5a78674c8ed1daa55435b56b60b4`.
The ordinary ARM64 smoke guest enters NtContinue at observer turn 2,943,204 with
TestAlert=1, valid flags 0x400017, target PC 0x7fff38b24d30 and SP 0x93ffc0. The
same turn restores PC/SP and GPR0=0x140001048, with no queued APC, frame or wait
status. The restored GPR0 remains the supplied entry argument rather than a
fabricated success status. The guest exits with code 0 at turn 3,115,529:
3,115,529 - 2,943,204 = 172,325 additional observer turns. Native and emulated
smoke stdout/stderr are empty and both exit 0.

Whoami remains at unsupported NtQueryVirtualMemory class 3, turn 2,966,197;
its native control exits 0 and prints the retained SYSTEM identity. Source,
core, observer, executable, program and six runtime DLL identities match before
and after both owning runs. Private byte copies preserve the exact owning
archives and observer executable before later builds reuse their output paths.
These results establish one ordinary Windows smoke completion. The complete
process objective remains active.

The raw prototype, compiled baseline, same-height ownership regression and
controller corrections are preserved in `validation/`. An independent native
query confirms Windows version 10.0 build 29683 and native architecture ARM64.
Assumptions A1-A4, change surfaces, bounded findings and Quality Gates accompany
the exact acceptance ledger. Run `python3 -B check_validation.py --host HOST`
with HOST macos, linux or windows. Native materialization permits only the
recorded 374-byte offline Cargo configuration adaptation.

The actual retained Assist C++ `RaxProcess` adapter and C ABI also execute the
ordinary smoke using the same owning carrier: `RAX_PROCESS_EXITED` (4), code0,
empty stdout/stderr, 975 scheduler turns with 4096-instruction slices. Recorded
API execution time is 2,199,268 microseconds = 2.199268 seconds; this is one run,
not a benchmark. Its turn count uses a different slice size from the single-step
Rust observer and cannot be subtracted from that observer's count. Whoami returns
`RAX_PROCESS_FAILED` (5) with the same explicit class3 frontier. Native controls,
four committed adapter/header input identities, source, carrier, executable,
programs and six runtime DLLs match before and after the C++ runs. Source blob
references identify the retained Assist implementation without copying it into
this repository. The first controller's compiler-lookup failure is preserved;
the corrected controller builds with the absolute captured compiler path.

Twenty final logical corruptions, including the two C API boundary controls,
are rejected in addition to the thirteen native-record controls. Initial
portable verification passes on Windows, macOS and Linux; the final expanded
proof also covers the C++ boundary execution.

## Current epoch3 validation

A later exact-target regression compiled and failed 0 passed / 1 failed: a
callback that restores its exact interrupted PC/SP must abandon its APC owner.
Native supplemental controls complete 16 additional resumes, 32 post-checks and
28 callbacks across the same four ABI profiles. Together with the base corpus,
the native boundary contains 68 resumes, 136 post-checks and 76 callbacks.

Descriptor identity alone proved insufficient in the next broad run: the
existing interrupted-wait test failed. The current rule requires both the
private APC descriptor and an active callback stack marker. It clears abandoned
APC callbacks while retaining a suspended wait parent and the checked retry
frontier. Both native and HLE exact-target cases, plus native/WoW64 CPU callbacks
that invoke the installed NtContinue leaf themselves, are covered. All rejected
attempts remain under validation/epoch3; none is used as current passing proof.

Concurrent upstream `b79c32ca2839568f5c06d123d2f8243e490ba2a4` changes nine
workflow/tooling/reference paths. Runtime source and all twelve owned feature
paths are preserved. Fresh current-source results are:

| Host | Library passed | Failed | Ignored | C API passed | Integration passed | All targets |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| macOS | 8082 | 0 | 2 | 183 | 544 | pass |
| Linux | 8076 | 0 | 2 | 183 | 544 | pass |
| Windows | 7543 | 5 established lowerer failures | 2 | 183 | 4 | pass |

The complete compilation graph has 25,071 files / 1,834,290,186 bytes. The owning
closure has 3,812 inputs. Seven macOS CTests, five C++ consumers on Linux and
Windows, actual archive member walks, stable before/after identities and six
macOS linker commands pass. The combined prototype checks all twelve shared
continuation cases and interrupted wait preservation together; native focus
executes thirteen checks. Both compiled case datasets and actual installed
native/WoW64 leaves remain part of current validation.

Current owning execution again completes ordinary ARM64 smoke with code0 after
NtContinue restores the exact PC/SP/entry registers. Whoami remains at class3.
The retained Assist C++ RaxProcess adapter and C ABI independently return
EXITED(4)/code0 for smoke and FAILED(5)/class3 for whoami. Current source, core,
carrier, observer, program, executable, native controls and six runtime DLL
identities remain stable. Exact current artifacts/outcomes are recorded in
validation/epoch3/summary.json and the raw receipts.

Historical epoch1 source receipts are checked against their explicit before /
after source overlay; they do not substitute for fresh epoch3 compilation or
execution. A5/A6, current Quality Gates and bounded findings accompany this
ledger. Whole current-root products remain separate from retained ABI184
consumers; the current publication base has Rust ABI270 and concurrent unrelated
changes. The full process objective remains active.

The evidence directory pins Git text conversion off so native CRLF logs and
all manifest-bound bytes survive a Windows checkout. Native source trees are
materialized from their exact source manifests before compilation.

The additional ownership pruning takes O(F) time for F abandoned frames and
O(1) auxiliary space. The alert decision is O(1); callback execution and queue
storage retain their existing scheduler bounds. Twelve additional corruption
controls check the current ledger with relevant log/receipt hashes rebound.
