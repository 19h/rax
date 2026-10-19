# Native dormant WorkerFactoryIdleTimeout (2026-10-19)

This group implements measured `NtSetInformationWorkerFactory` class 2 for
process-local dormant guest worker factories. The independent observations use
Windows 10.0.29683 on one ARM64 kernel, with ARM64, x64 compatibility, x86 and
x86 LARGEADDRESSAWARE producers. They establish this installed-build contract;
behavior on other kernel builds and active-worker scheduling remain unknown.

## Measured contract

| Input ABI | Accepted `Length` | Input alignment | Captured bytes |
|---|---|---|---|
| Native ARM64 / x64 compatibility | Exactly 8 bytes | 4 bytes | 8 |
| WoW64 x86 / x86 LAA | At least 8 bytes | 4 bytes | 8 |

The WoW64 wrapper translates the captured `LARGE_INTEGER` into a fixed native
request. Length `0xFFFFFFFF` does not cause a span of that length to be read.
Readonly and 4-byte-aligned but 8-byte-misaligned inputs are accepted.

```text
length -> alignment -> 8-byte read -> typed handle -> right 0x4
       -> signed interval validation -> clamp -> shared object update
```

Nonnegative intervals return `STATUS_INVALID_PARAMETER` only after the handle
and access checks. A failed request leaves the prior normalized timeout intact.
Memory faults retain the existing one-shot guard behavior: the first reached
guard is consumed, a later guard is preserved, and misalignment visits neither
page. The 2,112 cross-page controls distinguish guard and no-access ordering.

`Object::WorkerFactory::idle_timeout` is `Option<i64>`, in signed 100 ns ticks.
`None` means no recorded guest request and does not invent a constructor default.
A successful request stores the negative interval clamped to:

```text
[-6,000,000,000, -10,000,000] ticks * 100 ns/tick = [-600, -1] s
          -300,000,000 ticks * 100 ns/tick       = -30 s
```

These integer conversions are exact. Signed clamping handles `i64::MIN` without
negation. Per-call capture, arithmetic and additional state use O(1) time and
space; the existing handle table lookup uses O(log n) time for n handles.
Aliases observe one object state and retain the completion-port/process
references until the final reference closes. Classes 3, 5, 13 and 14 retain
independent binding, hard maximum, flag-request and soft-request state.

The public Rust enum gains a field, so explicit Rust constructors/exhaustive
patterns need that field. The opaque C API remains 1.14.0. No C layout, feature
default, dependency pin, guest-host forwarding or active worker policy changes.

## Native evidence and regression

| Producer | Cases per ABI profile | Profiles | Total |
|---|---|---|---|
| v1: signed values, lengths, capture, handles and grants | 640 | 4 | 2,560 |
| v2: clamp boundaries, alignment and value/access priority | 1,068 | 4 | 4,272 |
| v3: cross-page protection and guard priority | 528 | 4 | 2,112 |
| Combined | | | 8,944 |

All producers seed flag request 1 and cover both ordinary-program stack pairs:
reserve/commit 1,048,576/4,096 bytes and 524,288/8,192 bytes. Basic queries before
and after each call bound all 128 captured bytes, native query extent 120 bytes
or WoW64 extent 96 bytes, untouched sentinel tails, zero workers and balanced
handles. Only the timeout field at offsets 16..23 changes on success. PE/source
and six runtime-module identities are recorded before and after execution.

`check_native.py` checks every status, normalized timeout, ordinal, input,
protection, query field and identity. Fourteen intentionally modified records
are rejected after their outer manifest hashes are rebound, including a changed
clamp result, WoW64 length, value/handle priority and second-guard consumption.

The tests-only baseline compiled and ran three functions: 0 passed, 3 failed
because class 2 was unsupported. The six shared tests then passed, including
replay of all 8,944 rows. Initial epoch-1 native compilation found four E0425
occurrences from three missing status-constant imports in the Windows-only leaf.
Epoch 2 adds those imports in a separate immutable source snapshot; the initial
source and failed record remain retained. No assertion was weakened. The final
native leaf runs installed native NTDLL and WoW64 entries, including x86's
20-byte return cleanup for four arguments plus return address.

## Final validation and ordinary programs

| Host | Passed | Failed | Ignored |
|---|---:|---:|---:|
| macos | 8037 | 0 | 2 |
| linux | 8031 | 0 | 2 |
| windows | 7499 | 5 | 2 |

The Windows failures match the exact five pre-existing lowerer failures. Each host also passed 183 C API tests; integration selections passed 544 on macOS/Linux and 4 on Windows, with cfg-empty counterpart binaries identified as such. All-target compilation passed. Retained C++ selections passed 7 CTests on macOS and 5 consumers each on Linux/Windows, with six actual macOS target link commands and protected CLI/MCP scans checked.

The original validation snapshot contains 24,313 files and 1,752,724,333 bytes.
The reviewed source view contains 24,312 files and 1,752,710,101 bytes: only
one generated CPython import cache is removed. [Review delta](validation/source-review-exclusions.json)
binds original-minus-cache equivalence; every compiled Rust/test/build input is identical. Final records
bind the entire snapshot, selected 554 source/reference inputs for retained
Assist consumers, real Cargo archive identities and member walks, actual linker
commands, and ordinary-program identities before and after observation. Retained
Assist Rust ABI 184 and selected C++ consumers do not establish whole current-root
GUI/plugin/package validation; eight C++ inputs were compared with root ABI 264.

The actual retained-owner RAX core is 503929600 bytes, SHA-256
`b912928105f461adf0bf338d7ade0815b1557f379d0fa21c2728a93856804835`. Both ordinary ARM64 guests return class2 status0 and retain idle_timeout Some(-300000000), then advance359turns: smoke2898814->2899173 and whoami2901099->2901458. The next stop is unimplemented NtQuerySecurityAttributesToken (0x167). Neither guest exits successfully; both native controls exit0. Read-only final diagnostics capture pseudo token handle -4 and requested WIN://SYSAPPID/WIN://PKG names.

Controller exit 0 is distinct from successful guest exit. The full same-host
Linux/macOS/Windows process-emulation objective remains active while either
ordinary Windows guest has not exited successfully.

## Provenance, assumptions and bounded scope

The pinned [PHNT declarations](phnt-ntexapi.h) name class 2 `LARGE_INTEGER` and
place `IdleTimeout` in `WORKER_FACTORY_BASIC_INFORMATION`; they do not specify
this kernel's complete validation order or clamp. The captured native executions
provide that evidence. [Retrieval metadata](primary-retrieval.json) records the
canonical URL/hash. PHNT's license is retained in the sibling
`../native-worker-factory-create/phnt-LICENSE`.

The initial [Assumption Register and surface map](idle-plan.json) are reconciled
in [final-assumptions.json](validation/final-assumptions.json). A1 is scoped to the
one installed kernel and dormant factories. A2 retains unknown constructor
policy. A3 is tested by exact shared/native replay of the cross-page controls.
The full-process frontier is a high-impact unfinished requirement. Five exact
pre-existing Windows lowerer failures and retained/current-product coverage are
recorded separately; they are not silently counted as passing tests.

Reproduce evidence verification with Python 3.9 or newer:

```text
python -B docs/specifications/windows/native-worker-factory-idle-timeout/check_native.py
python -B docs/specifications/windows/native-worker-factory-idle-timeout/check_validation.py .
```

Run from the exact RAX source epoch/owning commit recorded here. Later source
revisions deliberately fail the versioned source hashes. The native checker defaults to its own evidence
directory. Final proof receipts name their original checked envelope; later
receipt/envelope extension is recorded without claiming recursive verification.
