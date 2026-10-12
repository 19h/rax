# Installed NT worker-factory binding-count updates

Measured on Windows 11 ARM64 10.0.29683.1000, 2026-10-12. Eight actual native
producers cover ARM64/x64/x86/x86-LAA fresh validation and repeated transitions.
Installed NTDLL/kernel executes under SYSTEM/S-1-5-18. Compatibility execution
is distinct from physical Windows x64. Source/PE machine/LAA/log/DLL identities
are retained; proprietary libraries and producer binaries are not redistributed.

NtSetInformationWorkerFactory class3/WorkerFactoryBindingCount captures an
exact4 B ULONG before typed handle/right0x4 checks. Invalid length preserves
guards; capture faults precede handle errors, and guard faults clear the guard
once without state mutation. Readonly/unaligned/last-page inputs succeed;
inaccessible/cross-page inputs fail. Type mismatch precedes access denial.

BindingCount starts at zero in the measured dormant constructors. A zero
request sets0xFFFFFFFF. Nonzero ULONG bit patterns act as signed32 deltas,
with saturation at zero and0xFFFFFFFF. This is not an overwrite or modular
addition. The effective active-worker meaning of the all-bits count is unknown.

```text
class3 -> length4 -> capture ULONG -> typed factory -> right0x4
                                                        |
                                                        v
                          v=0: B'=0xFFFFFFFF
                          v!=0: B'=clamp(B+sign_extend_i32(v),0,0xFFFFFFFF)
```

| Original independent capture | Count |
|---|---:|
| Values per profile |11 |
| Buffer/protection/length partitions per profile |72 |
| Eighteen handle/type/access/capture/length cases per profile |216 |
| Fresh validation across four profiles |4*299=1196 |
| Repeated boundary transitions across four profiles |4*26=104 |
| Total direct native replays |1300 |

Class7 basic-query bytes32..36 expose BindingCount. Every other backing byte,
hard maximum512, zero TotalWorkerCount and handle lifetime remain unchanged.
Query requires exact120 B/native64 or96 B/WoW64 in128 B backing with untouched
sentinel tail. Guest NtQueryInformationWorkerFactory is not added by this group.
The transition sequence covers positive accumulation, signed negatives, lower
and upper saturation, zero and updates after the all-bits count.

The public Rust WorkerFactory variant gains binding_count:u32; constructors and
exhaustive consumers are updated. Aliases share the same count. Failed captures,
type/access checks or lengths preserve the previous count and all hard/soft/
entry/stack/reference state. C API1.11.0 and Assist Rust ABI184 are unchanged;
opaque process interfaces do not expose Object layout. No host workers start.
Guest worker activation, other information classes and ordinary program exit
remain separate incomplete contracts.

For0<=B<=2^32-1 and signed32 d, B+d lies in[-2147483648,6442450942], fitting
signed64. The implementation uses u32.saturating_add_signed after the explicit
zero case. Counts are dimensionless; input4 B, x86 caller cleanup4*4 B+4 B=20 B.
Handle lookup O(log H), object access expected O(1), arithmetic/space O(1).

Pinned PHNT declares the class name and ULONG payload, rather than kernel
arithmetic/order. Its header/license/retrieval record remain in
[constructor provenance](../native-worker-factory-create/primary-contract.md).
[Primary declaration](https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntexapi.h).
Exact state/status behavior is original independent native evidence.
`scope.md` contains the assumption register, change planes and bounded findings.
Before production changes the observed baseline is1 pass/4 class3 failures.

## Final validation

| Gate | macOS | Linux | Windows |
|---|---:|---:|---:|
| Focused binding tests |6 |6 |7 |
| Full library passed/failed/ignored |7653/0/2 |7645/2/2 |7055/5/2 |
| C API passed |168 |168 |168 |
| Registered integrations passed |544 |544 |4 |
| Owning C++ consumers passed |5 |5 |5 |
| All-target build |pass |pass |pass |

Fourteen selected gates per host include prior hard/soft setters and constructor,
association, threading and event regressions. The original two Linux timeout
failures pass isolated; Windows retains four BZHI and one FP16 lowering failure.
No assertion or skip is weakened. Unix C API and Linux integrations are serial;
Linux amd64 CPU execution is translated. macOS CLI/MCP, seven CTests and both
protected-text scans pass. Unix owning root e2fc5b48 matches compiled root
3c707be3 in owning source/config bytes except two records/gitlink. Windows
verifies16857 files/135 patches and the unchanged374 B offline adapter. Its
static-CRT Assist/core archives1108659444 B/721826134 B pass4673/260-member
walks, with43 source/two helper identities and empty core features. Finalized
prose/evidence is separate from the compiled snapshot. Physical x64 Windows,
packages, live IDA and Qt harness are not established.

The identical11177 B observer advances42802->44258, exactly1456 scheduler
calls, from class3 to NtSetWnfProcessNotificationEvent0x1C5 atPC0x180002C90,
thread8, event handle0x34. All four ordinary smoke/MSVCRT/UCRT/installed-whoami
programs still report reason5/exit field0/internal failure before guest exit.
Full process and worker-factory activation/completion remain incomplete.

Run `python3 check_native_worker_binding.py` here. Historical validation needs
a checkout matching all43 frozen source hashes. The checker binds eight actual
producers, all1300 raw case/state observations, observed1-pass/4-failure baseline,
nonempty host/owning results and the byte-identical observer. Six controlled
negatives cover manifest omission, raw corruption, rehashed invalid native
status, empty filters, forged summaries and optimized Python.
