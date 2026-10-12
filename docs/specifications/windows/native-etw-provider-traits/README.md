# Native EtwSetProviderTraits (NtTraceControl function 30)

This increment captures provider traits once on the existing process-local ETW
registration, using measured Windows 10.0.29683.1000 behavior. The unchanged
ordinary observer advances from scheduler turn 50,692 to 90,924:
90,924 - 50,692 = 40,232 scheduler calls. All four ordinary programs now reach
unsupported `NtOpenDirectoryObject` service 0x58 at PC 0x1800015C0 before guest
exit. Complete Windows application execution remains unfinished. This is an
engineering evidence record.

| Private input span | Measured representation |
|---|---|
| [0,8) | 64-bit registration handle, including WoW64 callers |
| [8,16) | 64-bit referenced traits address, including WoW64 callers |
| [16,18) | USHORT capture length T, 1..65535 B |
| [18,24) | Six ignored bytes |

Input must be exactly 24 B and the required output buffer at least 120 B.
Full nonempty optional output write probing precedes the required 4 B
ReturnLength write probe. Full nonempty non-NULL input read probing precedes
class-specific length checks. The header is captured before state or stores.
A NULL referenced pointer or zero effective USHORT length fails with
STATUS_INVALID_PARAMETER before handle lookup. Bad handles and wrong object
types fail before referenced-buffer access. An already assigned registration
returns STATUS_UNSUCCESSFUL (0xC0000001) before probing that referenced buffer.

The bounded structural parser requires a matching USHORT total length, a NUL
name within the capture and exact exhaustion of records. Each record contains
a USHORT size, one type byte and optional payload, with size at least 3 B.
The measured kernel accepts non-UTF-8 name bytes and unknown record types;
their bytes are retained. It does not impose known-type GUID payload validation
in these observed calls. Malformed structure returns STATUS_FILE_CORRUPT_ERROR
(0xC0000102), translated by installed RTL to ERROR_FILE_CORRUPT (1392).
Installed RTL preserves STATUS_GUARD_PAGE_VIOLATION (0x80000001) verbatim.

Successful calls retain an immutable owned capture and then store ReturnLength
zero. The output is probed but has no reply bytes in the measured dormant
profile. Alias tests pin capture before the ReturnLength store, including
overlap with the private handle, referenced pointer, length and trait bytes.
Faults and malformed captures publish no traits. A valid retry succeeds after
failure; assignment repeats fail. Caller writes cannot alter the stored capture.
Final close, lowest-free handle reuse and object-table drain release the state.
Functions other than 15/27/30 retain explicit unsupported results. No host ETW
forwarding, guest controller enable policy or notification delivery is added.

Capture/validation takes O(T) time, O(T) retained space and O(1) parser auxiliary
space, with T <= 65535 B. Each registration holds its original 160 B request
plus the bounded traits capture and object-table metadata. Allocation failure
returns STATUS_NO_MEMORY before publication.

| Native family | Cases/profile | Profiles | Complete cases |
|---|---:|---:|---:|
| Input/output lengths, names, embedded size, handle types, ignored bytes | 1032 | 4 | 4128 |
| Base protection, guard, alignment and probe precedence | 5600 | 4 | 22400 |
| Structural names, records, types, tails and full 65535 B capture | 711 | 4 | 2844 |
| Referenced pointer/protection, wrong type, once-only state and RTL mapping | 990 | 4 | 3960 |
| ReturnLength/header/traits/output aliases | 864 | 4 | 3456 |
| High DWORD lengths and effective USHORT capture | 210 | 4 | 840 |
| Fresh original entry/return and installed payload capture | 2 | 2 ARM64 producers | 4 |
| Total | | 26 actual producers | 37632 |

There are 9407 mature cases per profile: 1032+5600+711+990+864+210.
Four profiles yield 37628 direct Rust replay rows, plus four fresh cases checked
by the evidence verifier. Two debugger warmups are excluded. Mature profiles
are ARM64, x64 compatibility, WoW64 x86 and WoW64 x86 LAA on one ARM64 kernel.
Fresh captures use installed MEM_IMAGE NTDLL bytes and original entry/true-LR
return breakpoints. The fresh installed 32 B payload is USHORT 32 followed by
`EtwUserModeDiagnosticProvider` and its NUL terminator. Owned debugger children
terminate with exit 77 after the observed calls and restore their native handle
sets; this is separate from ordinary guest exit. Fresh compatibility captures
and physical x64-kernel proof remain unknown.

Primary declarations and public-format sources:
[PHNT ntwmi.h at 53fbbdc5](https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntwmi.h),
[Microsoft provider traits](https://learn.microsoft.com/en-us/windows/win32/etw/provider-traits),
[Microsoft EventSetInformation](https://learn.microsoft.com/en-us/windows/win32/api/evntprov/nf-evntprov-eventsetinformation).
Private validation order and acceptance come from the retained native corpus.
The pinned header/license, sources, actual native source/PE/log identities,
installed System32/SysWOW64 NTDLL identities and original stdout are retained.
Format payloads have independent FNV-1a 64-bit hashes and exact prefix/suffix
checks; producer and replay generators reconstruct the full tested bytes.

| Host | Focused passed | Full passed/failed/ignored | C API passed | Integration passed | C++ consumers |
|---|---:|---|---:|---:|---:|
| macOS | 8 | 7699/0/2 | 168 | 544 | 5 |
| Linux | 8 | 7693/0/2 | 168 | 544 | 5 |
| Windows ARM64 | 9 | 7106/5/2 | 168 | 4 | 5 |

All 19 gates per host complete, including the prior ETW registration,
notification, WNF and kernel-service regressions. The earlier registration
filter includes the new nested traits tests: 17 passes on Unix, 19 on Windows.
Full suites are unfiltered; C API suites run serially. Windows retains four
BZHI lowering and one FP16 runtime failures in untouched code. Linux's earlier
io_uring timeout failure is absent in this final full run. No assertion is
weakened and no platform skip is added. Initial replay failures are preserved:
native high-length evidence corrected the initial DWORD assumption to USHORT;
the native RTL mapping corrected guard status translation from 317 to raw
0x80000001. Both corrections precede these final validation runs.

Mac owning CLI/MCP, seven selected CTests and two protected-text scans pass.
Linux owning shipping archive and five C++ consumers pass on a translated amd64
CPU with a native Linux kernel. Windows builds the static-CRT Assist archive,
runs the same five owning C++ consumers and uses the current empty-feature
core artifact selected from Cargo JSON for the unchanged ordinary observer.
Source acquisition verifies 17883 files and 218 patches with zero mismatches
and the unchanged 374 B offline source adapter. The Assist archive is
1118463976 B with 4673 completely walked members; the core rlib is 731186588 B
with 260 walked members. Actual archive/source identities bind 68 source units
and two helpers. Core features are empty.

Unix owning builds use frozen root e2fc5b48: the recorded 11-file delta includes
two engineering records and nine concurrent masking files, plus the gitlink.
These builds do not validate the masking changes. Windows acquisition includes
committed root f386ed2a. Public C API/Assist Rust ABI, C layouts, defaults,
dependency pins and package layout are unchanged. The public Rust
`Object::EtwRegistration` variant gains the `traits` field. Entire packaging,
live IDA and Qt harness runtime proof is not claimed.

Run `python3 docs/specifications/windows/native-etw-provider-traits/check-evidence.py`
from the RAX root. The checker rejects optimized Python, verifies exact manifest
coverage, complete partitions and independently derived statuses/captures,
guard/resource restoration, actual native producer identities, build/test
counts, archive identities and current compiled-source hashes. The negative
runner rejects six controlled corruptions for their intended reasons,
including rehashed semantic rows, forged PE architecture and altered compiled
source. Negative copies remain in the external task cache; only results/logs
are retained here.

| ID | Dependent result | Stress test / falsification | Status |
|---|---|---|---|
| A1 | Private function30 behavior on installed build | Guards, aliases, lengths, records; another installed build differs | Confirmed for measured build only |
| A2 | Dormant captured bytes without tracing controllers | Enable a reached guest controller requiring absent policy/delivery | Retained; controller/delivery unsupported |
| A3 | Fresh compatibility assignment follows ARM64 | Fresh x64/WoW64 original entry and true-return capture differs | Retained; fresh compatibility unknown |

`assumption-register.json` contains basis and concrete probes.
`change-surface.json` maps affected planes and unchanged owning contracts.
High remaining work is NtOpenDirectoryObject/later services and complete
ordinary guest exit; reached tracing controllers require actual enable state
and delivery. Medium proof limits are fresh compatibility/physical x64,
untouched Windows full-suite failures, concurrent masking coverage and full
package/live IDA/Qt runtime. Non-blocking findings remain outside this increment.

QG1 requires no normative conclusions. QG2 supplies assumptions and probes.
QG3 covers this traits increment while full-process acceptance stays open.
QG4 uses byte spans, exact partitions and reproducible scheduler arithmetic.
QG5 reconciles measured USHORT/guard/alias/once-only edges and retains explicit
unsupported boundaries. QG6 binds primary declarations, actual native evidence
and owning archives. QG7 records impact and proof limits. Positive verification
and all six controlled negative rejections must pass before exact-file
publication through red.
