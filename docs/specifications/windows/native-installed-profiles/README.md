# Native installed-library test profiles

Installed-leaf tests select the current Windows DLLs. The recorded capture's
build, SYSTEM identity and session zero describe its corpus; they are not
requirements on the machine executing that corpus through installed stubs.

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
| CI-P1 | An installed leaf can replay an explicitly selected frozen subject/session. | DLL instruction/ABI selection and virtual token inputs are separate contracts. | Exact portable matrix replay. | Non-SYSTEM identity and nonzero session across available native/WoW profiles. | Any unchanged golden status, output byte or fault assertion differs. | Retained; recorded ARM64/WoW cases pass, hosted x64 pending |
| CI-P2 | The tested shared NLS/console semantics apply beyond capture build 29683. | Services are selected from actual installed exports; frozen replay retains its capture metadata. | Installed tests use current build control. | Native ARM64 build 29683 and hosted x64 build 20348. | Any unchanged semantic or ABI assertion differs. | Retained pending hosted validation |
| CI-P3 | Primary-token fields remain stable between selection and the independent control. | Fixed read-only queries; production acquisition already repeats captures. | Exact live field comparison. | Concurrent external privilege/group changes. | Repeated native controls differ or stable controls disagree with capture. | Retained; recorded native controls agree |

High impact: the clock failure's cause remains unknown until measured samples
and selected leaf bytes establish it. Rust 1.99.1 in the failing CI job uses
the same precise Windows clock API as the adapter; an API-family difference
does not explain the failure.

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
