# Dormant WorkerFactoryFlags requests

`NtSetInformationWorkerFactory` information class 13 accepts the measured
four-byte `ULONG` request for a typed dormant factory with access bit `0x4`.
The shared service preserves the installed validation order: exact length,
input capture, typed handle, then access. Read-only and unaligned inputs work;
inaccessible/cross-page inputs fail; guard capture consumes the guard once.
Invalid length preserves the guard. Failure leaves the prior request and
constructor/reference state unchanged. The native/WoW64 transport still has
four arguments; the actual x86 leaf returns over `4*4 + 4 = 20` bytes.

`Object::WorkerFactory.flags_request: Option<u32>` records the last successful
opaque request. `None` means no guest request has been recorded. It does not
declare a native constructor flag default. `Some(0)` is distinct from `None`.
Aliases share the record; hard maximum, soft-limit request, binding count,
entry/parameter, stack fields and internal completion-port/process references
remain independent. This Rust enum source shape requires updated constructors
and exhaustive matches; the opaque C API 1.14.0 and Assist binary interfaces
are unchanged. Handle lookup remains O(log H), object lookup expected O(1),
input capture four bytes, and added request state O(1).

Native evidence contains 5,236 observations over four ABI profiles on one
ARM64 Windows kernel. The main 2,600-row matrix uses hard maxima 1/512 and
reserve/commit 1,048,576/4,096 bytes. It covers 37 flag patterns, buffer lengths,
protection/alignment/page boundaries and handle/type/access priority. All tested
unsigned patterns succeed with valid input/access, including every one-hot
bit, zero and `UINT32_MAX`. Thirty-six native controls additionally set the
hard/soft/binding properties before class 13. All captured basic-information
backing bytes are unchanged by class 13, and all worker totals remain zero.

A separate 2,600-row supplement uses the second ordinary guest's observed
reserve/commit of 524,288/8,192 bytes. Its identical request/error/basic-state
contract is checked independently without changing the frozen compiled inputs.
Both producer families retain source, PE machine/LAA, exit status, native DLL,
handle balance and raw backing-buffer evidence. Twelve rebound native
corruptions are rejected. `check_supplement.py` covers the supplemental stack
controls, and validation-negative controls also alter a rebound supplement.

The pinned [PHNT declaration](https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntexapi.h)
defines the class name, `ULONG` payload and four-argument signature. It does
not guarantee kernel behavior. The retained header, retrieval record and
[applicable license](../native-worker-factory-create/phnt-LICENSE) provide
declaration provenance; statuses and ordering come from the original native
controls. Active-worker effects and the native constructor flag default
remain unknown. This group implements dormant request capture and bookkeeping,
without claiming complete worker activation or effective flag policy.

The corrected tests-only baseline compiles and fails three assertions on the
previous explicit unsupported class. Baseline A's fixture shadowing compile
error remains preserved and is excluded from regression evidence. Six shared
tests cover all 2,600 native rows, error priority, alias/reference lifetime,
guard retry, independent state and failure atomicity. An additional Windows
test executes the installed native and WoW64 setter leaves and verifies
actual return status and caller stack cleanup.

The source epoch binds 24,045 files and 1,731,467,640 bytes. Full library/C API,
integration, all-target and retained-consumer gates are recorded under
`validation/`. Retained Assist Rust ABI 184 and 540 selected inputs do not
establish a full current root GUI/plugin/package build. The actual retained-owner
core is ACTUAL_CORE_BYTES bytes, SHA-256 `ACTUAL_CORE_SHA`.

Both ordinary guests return class-13 success, retain value `1`, and advance
another 496 normally executed observer turns to unsupported class 2, the
worker-factory idle-timeout setter. Native controls exit successfully; neither
emulated guest demonstrates successful exit. The full same-host process
objective remains active. Assumptions, bounded findings, exact known Windows
failures, quality gates and verification-receipt scope remain under `validation/`.

```text
python3 -B docs/specifications/windows/native-worker-factory-flags/check_native.py
python3 -B docs/specifications/windows/native-worker-factory-flags/check_supplement.py
python3 -B docs/specifications/windows/native-worker-factory-flags/check_validation.py .
```
