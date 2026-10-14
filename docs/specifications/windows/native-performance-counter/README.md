# Native performance-counter evidence

Implementation and final scoped validation completed on 2026-10-14. Full ordinary Windows
process completion remains unproved. Root GitLab publication of the preceding
mapping increment remains pending because network connections time out.

## Contract

`NtQueryPerformanceCounter` appends native export index42 after the previous42
entries. It accepts two guest pointers; the counter is mandatory and frequency
is optional. Each result is an8-byte LARGE_INTEGER on ARM64, x64 and x86.
The measured output alignment is4bytes. Counter publication precedes optional
frequency validation/store. A later frequency fault retains the counter.
Overlapping outputs observe counter-first, frequency-second stores.

| Outcome | NTSTATUS | Caller/guard effect |
|---|---|---|
| Accepted counter and optional frequency | 0x00000000 | Counter, then supplied frequency |
| Output address not divisible by4 | 0x80000002 | Reached output fails; previous counter retained |
| Writable guarded destination | 0x80000001 | One reached guard consumed |
| Null mandatory, reserved, no-access or read-only destination | 0xC0000005 | Rejected store writes no bytes |
| Read-only guarded destination | 0xC0000005 | Guard remains armed |

Each caller store checks its complete8-byte span. The two-output operation is
not transactional. Cross-page failures preserve canaries; successful partial
aliases are compared byte for byte.

The Windows personality uses one PerformanceClock for its NT service, public
Kernel32 counter/frequency handlers and readonly KUSER_SHARED_DATA.QpcFrequency
at0x300. Native installed-library mode selects the existing user::clock adapter's
raw host counter ticks and cached positive frequency. Only initialized host-owned
LARGE_INTEGER storage enters the supported host Kernel32 APIs. Guest pointers,
addresses, flags or handles are never forwarded. Host failure remains an error.
The existing host monotonic nanosecond reader delegates the same raw sampler and
retains its widening/floor conversion. Unix clock_gettime branches are unchanged.

Built-in Windows mode retains its elapsed-nanosecond counter and1000000000Hz
frequency. Its origin is process start; native mode uses the host counter epoch.
A nanosecond unit is not a nanosecond precision claim. The24MHz native captures
represent ticks, not100ns units:24,000,000counts/s gives a nominal tick interval
of1/24,000,000s, approximately41.667ns. Actual clock accuracy is not inferred.

The shared handler's address-policy lookup costs O(log P) for P VM policy runs,
with at most two pages per8-byte store. Clock sampling and argument/store policy
otherwise cost O(1); retained per-process clock metadata is O(1).

## Independent native controls

MSVC18.7.1 compiled static-CRT C++20 controls for ARM64, x64, x86 and x86 LAA on
the same native ARM64 Windows10.0.29683 kernel under SYSTEM, S-1-5-18. Actual
source, executable PE Machine/LAA bits, bytes, SHA-256, stdout and producer exit
identities are retained. Physical AMD64/x86 kernels and other builds/subjects are
unmeasured. x64/x86 controls establish compatibility-process behavior on ARM64.

| Per-profile controls | Count |
|---|---:|
|15counter x15frequency pointer matrix |225|
| Forward aliases, offsets0..12 |13|
| Reverse aliases, offsets1..12 |12|
| Total calls/profile |250|
| Four-profile calls |1000|
| Monotonic observations/profile |8|
| Four-profile monotonic observations |32|

All four calibrations reported24000000Hz and exactly matched public
QueryPerformanceFrequency. No collector exception escaped. The caller arena
reserves16pages and commits8, so boundary probes cannot write another host
allocation. VirtualQuery records guard/protection after each call; the collector
then restores its own page access to capture every changed byte. Native
executables and DLL bytes are not redistributed.

`verify-evidence.py` binds recorded source/log bytes, source freeze, actual
executable identity/outcome, PE Machine/LAA, kernel/subject/library identity,
ordered complete rows, changed-byte extents, clock brackets and monotonic deltas.
All four profiles' status/guard matrices must match on the measured kernel.
Ten controlled corruptions are rejected, including rehashed missing-row, status
and monotonic changes, executable/source/library/subject/kernel/frequency
identity changes and optimized Python. [The rejection report](evidence-stress-results.json)
records their exact reasons. Optimized Python is rejected. Primary API links, PHNT header identity and the
QpcFrequency offset declaration are in [primary-contracts.json](primary-contracts.json).
The misleading historical PHNT100ns counter-unit comment is superseded for this
measured runtime by the documented Hz contract and actual24MHz calibration.

```sh
python3 -B docs/specifications/windows/native-performance-counter/verify-evidence.py
python3 -B docs/specifications/windows/native-performance-counter/check-final-validation.py
cargo test --locked --offline --no-default-features --lib performance_ -- --nocapture
```

## Runtime validation and scope

Portable strategic regressions cover all three argument transports, optional
frequency, exact alignment, aliases, partial publication, readonly/guard policy,
cross-page rejection, shared frequency and the preserved built-in clock.
The1000-call replay injects a test-only captured clock value within the native
before/after bracket. Every visible counter byte constrains that value. Aliases
can overwrite some counter bytes with frequency; failed counter stores retain
exact canaries. Status, guard state and all32768caller bytes are compared exactly.
This normalizes a runtime-varying clock sample; it does not prove host clock
sampling. Separate native tests compare the actual host frequency/epoch directly
and execute installed NTDLL and public Kernel32 helpers on native/WoW64 paths.

The first compile failed because the new tests lacked a layout import; corrected
logs are retained. Focused gates passed7tests on macOS/Linux and8on Windows,
including one pre-existing shared-clock conversion test. The first broad macOS
and Linux library runs passed7770/7764 respectively, with2ignored each.
Windows recorded7185passes,6failures and2ignored: the5preceding AArch64 baseline
failures plus an existing CPU-attribution fixture failure. A caller tick of
15.625ms equalled one-quarter of the62.5ms worker, violating its strict ratio.
The fixture now finishes thread/CRT startup before measurement and burns at
least250ms worker CPU. The same strict attribution assertion remains. Refined full gates pass7770/0/2 on macOS,7764/0/2 on Linux and7186/5/2 on
Windows (pass/fail/ignored). Only the5preceding Windows failure identities
remain; the failed original observations are retained.

Scoped owning builds use frozen Assist roots e2fc5b48 on Unix and f386ed2 on
Windows,210scoped Rust source units,9byte-matching current C++ consumers and
empty core features. They are not whole-current-root/plugin/GUI/package proof.
Final archive identities match the refined source freeze and210scoped Rust
source hashes. C API168, all-targets, host-selected integration and all5scoped
owning C++ consumers pass per host. macOS additionally passes2manifest CTests
and2protected-static-text scans.

The unchanged11177-byte observer exhausted its1,000,000-turn budget after the
counter implementation. A derived observer changes only that budget and final
index to3,000,000turns; it reaches NtSetInformationProcess class35 at turn2110676.
The C++ ordinary probe independently reaches class35 on three CRT programs;
whoami retains NtTraceEvent0x5E. All four remain Internal;0guest exits are proved.
The extended observer is compared to the original by reversing exactly the two
budget-literal changes, and is compiled against the same verified owning rlib.
A larger diagnostic budget changes evidence reach, not guest runtime behavior.

## Assumptions and bounded findings

[assumption-register.json](assumption-register.json) records the clock units,
sequential store order, guard policy, observed boundary and tick normalization
with falsification probes. [change-surface-map.json](change-surface-map.json)
covers the16execution planes. The known high-impact remaining requirements are
class35 support, NtTraceEvent, mixed ARM64EC/x64 execution and full ordinary
Windows completion. Clock resolution/timing-fixture effects are measured limits,
not grounds for claiming a failed gate passed. Final source/runtime evidence is reconciled in
[validation-summary.json](validation-summary.json); exact commit and remote
publication are audited outside this committed corpus to avoid self-reference. No public C ABI, dependency pin, default feature or package contract
has changed.
