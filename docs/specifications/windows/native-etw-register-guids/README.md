# Native EtwRegisterGuids (NtTraceControl function 15)

This increment implements measured dormant process-local ETW provider
registration on Windows 10.0.29683.1000. It does not establish complete Windows
application execution. The unchanged ordinary observer advances from scheduler
turn 50,306 to 50,692 (50,692 - 50,306 = 386 calls) and now reaches unsupported
function 30, EtwSetProviderTraits. All four ordinary programs still stop before
an actual guest exit. This is an engineering evidence record, not release notes.

The measured contract requires exactly 160 B input and at least 160 B output.
Full nonempty optional output write probing precedes required 4 B ReturnLength
write probing. Nonempty non-NULL input is read-probed before class-specific
length checks. Successful requests are captured before publication. A distinct
`EtwRegistration` resource retains the 160 B request and grants access 0x804.
The reply copies that request, replaces bytes [24,32) with the zero-extended
handle, stores DWORD 160 at [44,48), and zeros [112,160). The private meaning of
that final span is unknown; its measured dormant contents are reproduced.
Reply copying precedes the ReturnLength store, including overlapping aliases.
Failed preconditions preserve bytes and resources; final close and table drain
release registrations. Function 27's existing Event pin remains independent.
No host ETW forwarding or controller/callback delivery is introduced.

Object insertion has expected O(1) time, lowest-free handle selection O(H) time
for H examined handles, and retained state O(1) per registration: 160 B plus
object/handle entries. Repeated GUIDs create separate resources. Both same-GUID
and unique-GUID groups admitted 1,030 simultaneous registrations per profile;
the public EventRegister limit of 1,024 is not imposed on this private syscall.

DuplicateHandle does not duplicate this resource. Admissible calls with a valid
target fail with ERROR_NOT_SUPPORTED (50); access denial preserves the source.
CLOSE_SOURCE can still close a source after an admitted operation. Native64
preserves the source when output copying faults; measured WoW64 may close it
when rights are admissible and CLOSE_SOURCE is set. First-reached guard faults
consume the guard and preserve raw 0x80000001 in GetLastError. Extended option,
rights, NULL-output and crossing-buffer matrices pin these distinctions.

| Native family | Cases per profile | Profiles | Completed cases |
|---|---:|---:|---:|
| Lengths, fields, NULLs and in-place reply | 184 | 4 | 736 |
| Protection, guard, alignment and probe order | 5,600 | 4 | 22,400 |
| Aliases plus original duplication matrix | 264 | 4 | 1,056 |
| Same/unique GUID capacity and drain | 2,060 | 4 | 8,240 |
| Extended duplicate options/rights/targets | 288 | 4 | 1,152 |
| Duplicate output permissions/guards/crossings | 160 | 4 | 640 |
| Fresh original and pre-Event diagnostic registration | 2 | 2 ARM64 producers | 4 |
| Total | | 26 actual producers | 34,228 |

Two debugger warmups are excluded. Rust tests directly replay 33,744 rows;
four fresh cases and 480 repeated duplicate rows are independently checked by
`check-evidence.py`. The latter rows agree with the extended duplicate matrix.
Fresh original calls use installed NTDLL entry/true-LR return breakpoints.
Pre-Event registration redirects only an owned diagnostic child's first function
27 call through owned scratch; the ordinary execution observer remains unchanged.
Mature profiles are ARM64, x64 compatibility, WoW64 x86 and WoW64 x86 LAA on one
ARM64 kernel. This is not physical x64-kernel proof.

Primary declaration: [PHNT ntwmi.h at 53fbbdc5](https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntwmi.h).
The pinned header and license, producer sources, actual native source/PE/log
identities, installed System32/SysWOW64 NTDLL identities and original stdout
are retained. Private field meanings are not inferred from offsets or names.
The first matrix producer's mistaken duplication expectation is preserved as a
diagnostic attempt. Corrected producers record the measured denial explicitly.

| Host | Focused passed | Full passed/failed/ignored | C API passed | Integrations passed | C++ consumers |
|---|---:|---|---:|---:|---:|
| macOS | 9 | 7691/0/2 | 168 | 544 | 5 |
| Linux | 9 | 7684/1/2 | 168 | 544 | 5 |
| Windows ARM64 | 10 | 7097/5/2 | 168 | 4 | 5 |

All 18 gates per host completed; full suites are unfiltered. C API suites run
serially. The Linux io_uring update/remove timeout failure is in byte-identical
parent source and its exact isolate passes. Windows retains four BZHI lowering
and one FP16 runtime failures. These failures are recorded, not changed or
converted into skips. Initial compile and replay failures are retained alongside
the corrected final runs. Mac owning CLI/MCP, seven selected CTests and two
protected-text scans pass. Linux uses translated amd64 CPU with a native kernel.

Windows source acquisition verifies 17,676 files and 218 patches with zero
mismatch and the unchanged 374 B offline adapter. The final static-CRT Assist
archive is 1,122,182,874 B with 4,673 walked members; core rlib is 734,900,632 B
with 260 walked members. Actual archive/source identities bind 65 source units
and two helpers; core features are empty. Unix owning builds use frozen root
e2fc5b48: the exact 11-file delta includes these two engineering records and
nine concurrent masking files, plus the gitlink. Those builds do not validate
the masking changes. Windows acquisition includes committed root addf705e.
Public C API/Assist Rust ABI, C layouts, build defaults and dependency pins are
unchanged; the public Rust Object enum acquires the stated registration variant.
Entire packaging, live IDA and Qt harness runtime proof is not claimed.

Run `python3 docs/specifications/windows/native-etw-register-guids/check-evidence.py`
from the RAX root. The checker uses explicit exceptions, rejects optimized
Python, checks exact manifest coverage, complete partitions, semantic replies,
resource drain, actual build/test counts and current compiled-source hashes.
The controlled negative runner rejects six corruptions, including rehashed
semantic rows, forged PE architecture and a changed compiled source. Negative
copies remain in the external task cache; only results/logs are committed.

| ID | Dependent result | Stress test / falsification | Status |
|---|---|---|---|
| A1 | Private function15 contract on installed build | Guards, aliases, lengths, rights; another installed build differs | Confirmed for measured build only |
| A2 | Dormant zero reply tail; no controller delivery | Change header/GUID then enable a guest controller | Retained; private tail meaning unknown, controllers unsupported |
| A3 | Fresh compatibility registration follows ARM64 | Fresh x64/WoW64 original entry and true-return capture differs | Retained; fresh compatibility capture unknown |

`assumption-register.json` contains basis and concrete probes. `change-surface.json`
records affected planes and unchanged contracts. High remaining work is function
30/later services and complete ordinary guest exit; reached guest tracing control
would require actual enabled state and delivery. Medium proof limits are fresh
compatibility/physical x64, unrelated full-suite failures, concurrent masking
coverage, and full package/live IDA/Qt runtime. These are bounded findings rather
than an application-success claim.

QG1 requires no normative conclusions. QG2's material assumptions and probes are
explicit. QG3 covers this registration increment; full-goal acceptance remains
open. QG4 uses bytes, exact partitions and reproducible scheduler arithmetic.
QG5 reconciles measured ABI/guard/alias/duplication edges and preserves explicit
unsupported boundaries. QG6 binds primary declarations, native sources, outputs
and owning archives. QG7 records impact and scope limits. Positive and controlled
negative verification must pass before exact-file publication through red.
