# Native installed-library test profiles

Installed-leaf tests select the current Windows DLLs. The recorded capture's
build, SYSTEM identity and session zero describe its corpus; they are not
requirements on the machine executing that corpus through installed stubs.
Selecting current test metadata does not admit an unmeasured production
profile: NLS initialization acquisition and dispatch still require build 29683.

| Input | Exact recorded replay | Independent installed-host control |
| --- | --- | --- |
| Access-check subject | Select the recorded subject before replaying both complete matrices. | Compare current captured user, groups, restricted groups, privileges and AppContainer fields with typed Win32 token queries first. |
| Primary-token session | Select session zero before replaying every recorded output byte. | Compare live capture with `GetTokenInformation(TokenSessionId)` first; preserve the separate PEB/process-session equality. |
| OS build | Preserve the original build metadata in the capture. | Compare selected NLS/console profiles with current `RtlGetVersion`, retaining all service status, output, fault and stack-cleanup assertions. |
| NLS contents | Preserve the recorded replay cases. | Compare the mapped view with current `System32/locale.nls` bytes and the native default LCID; verify actual unmapping. |
| Win32u initialization | Retain the exact prerequisite-failure and preserved-state assertions. | A missing `NtGdiInit2` export must agree with explicit unavailable GUI metadata. That branch verifies state preservation and reports that no leaf executed. |
| System time | Retain exact fault/guard replay and the strict before/after clock bracket. | On bracket failure report ABI, build, all three FILETIME samples and the selected leaf's first 48 bytes. No tolerance or retry is introduced. |

The live token control uses native `repr(C)` records and ABI-derived offsets,
separate from the production capture parser. It validates SID extents with
`IsValidSid` and `GetLengthSid`. Query buffers are initialized, pointer-aligned,
bounded to 65,536 bytes and kept alive until every embedded SID is copied.
Acquisition uses a fixed current-process token with `TOKEN_QUERY`; guest values
never enter these queries. The test compares exact fields without printing
private host SID/group arrays. Attribute presence and the existing independent
attribute byte/ownership tests remain separate checks.

Excluding OS query cost, local conversion takes
`O(G + R + P + S)` time and `O(G + R + P + S + B)` space: `G` groups,
`R` restricted groups, `P` privileges, `S` copied SID bytes and a query buffer
`B <= 65,536 bytes`. Counts are bounded before traversal and every retained
pointer/record extent is checked against its owning buffer.

## Assumptions and boundaries

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| CI-P1 | An installed leaf can replay an explicitly selected frozen subject/session. | DLL instruction/ABI selection and virtual token inputs are separate contracts. | Exact portable matrix replay. | Non-SYSTEM identity and nonzero session across available native/WoW profiles. | Any unchanged golden status, output byte or fault assertion differs. | Confirmed for the recorded ARM64/WoW and hosted x64 executions |
| CI-P2 | Current test metadata alone permits NLS initialization beyond capture build 29683. | The earlier test correction selected the current build. | Installed initialization beyond that build. | Native ARM64 build 29683 and hosted x64 build 20348. | Actual dispatch or native-data acquisition rejects the selected build. | Falsified: both production paths explicitly admit only 29683; console tests pass on 20348 |
| CI-P3 | Primary-token fields remain stable between selection and the independent control. | Fixed read-only queries; production acquisition already repeats captures. | Exact live field comparison. | Concurrent external privilege/group changes. | Repeated native controls differ or stable controls disagree with capture. | Retained; recorded native controls agree |
| CLK-A1 | The older jumping clock export returns zero because guest shared time is unpopulated. | Hosted output is zero; the export begins with E9 and startup does not populate shared `SystemTime`. | A future shared-clock repair. | Selected x64 thunk body and a guest execution trace. | The bounded native thunk or guest trace reads another source. | Retained; causal trace pending |
| CLK-A2 | Native `NtQuerySystemTime` fits a precise-clock before/after bracket. | The emulator's existing adapter uses the precise API. | Interpretation of the installed clock assertion. | Independent native query and shared-page samples. | A successful native result lies outside that bracket. | Falsified on ARM64 build 29683: native and shared-page values match and precede the precise bracket |

High impact: the clock failure's cause remains unknown until measured samples
and selected leaf bytes establish it. Rust 1.99.1 in the failing CI job uses
the same precise Windows clock API as the adapter; an API-family difference
between those two callers does not explain the failure. The native NT query
itself can have different precision, as the new independent control demonstrates.

High impact: an older Win32u without `NtGdiInit2` needs another conversion
adapter for full GUI support. Passing its explicit unavailable-path test is not
GUI conversion or installed-leaf execution. The raw DPI setter, broader Windows
userland and shipping/runtime qualification also remain incomplete.

This group changes tests and their controls. Production service behavior,
permissions, native metadata acquisition, ABI, dependency pins and defaults are
unchanged. The qualification below covers these test inputs and controls.

## Recorded qualification

The final typed-control source passes all-target compilation on macOS ARM64,
Linux x64 in a translated container and native Windows ARM64 build 29683.
Native tests exercise the available ARM64 and WoW profiles; this is separate
from a native x64-kernel run. Counts below are passed/failed test executions,
with zero ignored tests in every selected suite. The follow-up Unix slice
repeats access-check replay after adding the Windows-only typed control.

| Source frame | macOS | Linux | Native Windows |
| --- | --- | --- | --- |
| Eight test files, before typed subject control | 40/0 | 40/0 | 52/0 |
| Final nine test files | 6/0 | 6/0 | 52/0 |
| Final all-target compilation | Pass | Pass | Pass |

The Windows supervisor held the original controller's process handle, checked
its image and creation time, then waited for its exit. It required all seven
original gates and exact prior source identities before replacing three inputs
and starting the final run. Executed controllers and both source frames remain
distinct records. Neither successful local clock slice explains the earlier
hosted build-20348 failure. That host and the missing-export branch require the
subsequent hosted validation; no full Windows-userland completion is claimed.

## Hosted failure diagnosis

[Windows x64 job 120837493257](https://github.com/HexRaysSA/rax/actions/runs/40082067045/job/120837493257)
at RAX `0ecda8c3` ran 721 selected process tests: 718 passed, three failed,
none were ignored, and 7,058 were filtered out. This improves the preceding
715/6 result but remains a filtered suite with failures. The failures are NLS
initialization on the unadmitted build 20348, a zero installed-clock result,
and an x86 missing-export assertion that incorrectly required a later failure
reason instead of the already validated WoW64 paired-context reason.

The corrected missing-export assertion reuses that earlier strict reason check
and preserves every state assertion. The NLS leaf helper now prints the actual
dispatch outcome. Production profile fences, service decoders and clock
tolerances are unchanged.

`tools/ci/windows_installed_probe_v2.py` runs before the Windows process suite. It
reports native OS/PE identity, at most four selected-image code blocks of at
most 64 bytes per export, signed E9/AArch64 B targets, native/precise/shared
clock samples, and a valid NLS call's mapping geometry/content hashes. Branches
outside the selected image, cycles and excessive traversal fail before another
read. This observation path does not grant service authority to a jumping export.
Reads use bounded `ReadProcessMemory`; NLS bytes are limited to 8 MiB. A bounded
pre-call enumeration establishes that the returned mapped allocation is new
before cleanup may unmap it. An existing or unproven allocation remains untouched
and fails the probe. No guest operand reaches a native call.
Reports contain relative offsets, scalar values and hashes rather than host
addresses or host token data. Excluding OS query cost, code traversal uses
constant time/space; NLS inspection uses `O(B + R)` time and space for
`B <= 8 MiB` and at most `R = 65,536` queried address-space regions.

The initial native ARM64 build-29683 measurement and the ownership-checked
follow-up both passed. Their 864,256-byte readonly mapped
view contains the 862,808-byte installed `locale.nls` plus zero padding; LCID is
1,033 and the ignored output sentinels remain intact. Three clock samples had
native NT time equal to shared-page time, outside the precise bracket. In the
first sample the difference was
`(134373095436910986 - 134373095436829102) * 100 ns = 8,188,400 ns`.
This establishes a precision distinction, not the cause of the hosted zero.
Older-build valid-call observations also cannot replace fault, alias, guard,
copy-order and mapping-protection measurements before production admission.

The diagnostic/assertion changes pass the following focused gates, with zero
failed or ignored selected Rust tests. The native Python column covers the
three new walker tests; the embedded interpreter lacks `_overlapped`, which
the unrelated existing runner tests import through `unittest.mock`. All five
Python tests remain required in hosted CI and pass on both Unix hosts.

| Gate | macOS ARM64 | Linux x64 container | Native Windows ARM64 |
| --- | --- | --- | --- |
| Selected Win32u/NLS Rust tests | 11 passed | 11 passed | 15 passed |
| All-target compilation | Passed | Passed | Passed |
| Python runner/walker tests | 5 passed | 5 passed | 3 new walker tests passed |

The first native controller failed before launching Cargo because its copied
environment path named the wrong recovery directory. Its failed record remains
distinct. After terminal-state and absent-process checks, a fresh controller
used the existing, hash-verified environment and completed all three gates.
These local checks do not qualify the older build's missing-export branch,
NLS initialization, clock, full GUI or ordinary application completion.

## Primary contracts

Token query rights and result sizing follow Microsoft's
[GetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation)
contract. Native record layouts are
[TOKEN_GROUPS](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_groups)
and
[TOKEN_PRIVILEGES](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_privileges);
[GetLengthSid](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-getlengthsid)
requires a valid SID. The build query uses
[RtlGetVersion](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-rtlgetversion).
The CI clock reference is the
[exact Rust 1.99.1 source](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/library/std/src/sys/time/windows.rs).
The diagnostic uses Microsoft's
[KUSER_SHARED_DATA layout](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/ns-ntddk-kuser_shared_data)
and
[VirtualQuery](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualquery)
region contract. Observed NT return values and selected instruction bytes are
native evidence, not a documented guarantee for other Windows builds.
