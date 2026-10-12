# Installed NT worker-factory soft-maximum requests

Measured on Windows 11 ARM64 10.0.29683.1000, 2026-10-12. Four actual
ARM64/x64/x86/x86-LAA executables call the installed NTDLL/kernel under
SYSTEM/S-1-5-18. Compatibility execution does not establish physical Windows
x64 behavior. Actual compiled source, PE machine/LAA, raw log and DLL identities
are retained; native binaries and proprietary libraries are not redistributed.

NtSetInformationWorkerFactory class14/WorkerFactoryThreadSoftMaximum requires
an exact4 B ULONG input and factory right0x4. Length precedes input capture,
which precedes typed handle/access validation. All eleven unsigned values,
including zero and UINT32_MAX, succeed. Readonly/unaligned/last-page inputs work;
unmapped/inaccessible/cross-page inputs fail. Guard capture returns
STATUS_GUARD_PAGE_VIOLATION and clears the guard once without a state update.
Invalid length preserves the guard. Type mismatch precedes access denial.
Additional factory rights0x10/0x20/0x40/0x80/0xC0 individually fail this setter.

```text
class14 -> exact length4 -> capture ULONG -> typed handle -> right0x4
                                                               |
                                                               v
                                                   record soft request
                                                   preserve hard maximum
```

| Independent rows per profile | Count |
|---|---:|
| Unsigned values |11 |
| Buffer/protection/alignment/page boundary and lengths |72 |
| Eighteen handle/type/access cases and capture/length precedence |216 |
| Total per profile |299 |
| Total across four profiles |1196 |

Every before/after class7 basic-query backing byte is unchanged, with hard
maximum512, TotalWorkerCount0 and unchanged handle counts. Query extent is
exact120 B/native64 or96 B/WoW64; the untouched128 B backing tail is verified.
This query observes hard/basic state. It does not expose the stored soft limit,
constructor soft default or active-worker effects; those remain unknown.

The guest retains the last successful unsigned class14 request in
WorkerFactory.soft_maximum:Option<u32>, independently from maximum. None means
no request has been recorded; it is not a guessed native default. Aliases share
requests. Faults/denials preserve the previous request and all constructor
parameters/dependency references. Interleaved class5/class14 tests check independent
state. Worker activation/effective soft limits are separate incomplete contracts.
Other classes still fail explicitly rather than manufacture successful behavior.

This public Rust enum shape changes exhaustive source construction/matching;
all in-tree consumers are updated. C API1.11.0 and Assist Rust ABI184 remain
unchanged: their opaque process interfaces do not expose Object layout.
Handle lookup is O(log H), object access expected O(1), input capture4 B and
additional per-call space O(1). The service still has four arguments; actual
x86 caller cleanup is4*4 B+4 B return=20 B.

Pinned PHNT supplies the enum name and four-argument prototype, not a kernel
behavior guarantee. Its full header/license/retrieval record remain in
[constructor evidence](../native-worker-factory-create/primary-contract.md).
[Primary declaration](https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntexapi.h).
Native statuses/order/basic state come from original independent captures.
`scope.md` records assumptions, change planes and bounded findings.

The baseline records three class14 failures and one passing unsupported-class
check before production changes. Final validation also checks successful request
retention, hard/soft interleaving, one-shot guard/retry, failure atomicity, alias
lifetime and actual installed native/WoW64 leaves.

## Final validation

| Gate | macOS | Linux | Windows |
|---|---:|---:|---:|
| Focused tests passed |5 |5 |6 |
| Full library passed/failed/ignored |7647/0/2 |7639/2/2 |7048/5/2 |
| C API passed |168 |168 |168 |
| Registered guest integrations passed |544 |544 |4 |
| Owning C++ consumers passed |5 |5 |5 |
| All-target build |pass |pass |pass |

There are thirteen selected gates per host, including unchanged hard-maximum,
association/constructor/threading/event regressions. Linux retains the same two
timeout failures, each passing isolated; Windows retains four BZHI and one
FP16 lowering failure. Original full outputs remain. No assertions/skips are
weakened. Unix C API and Linux integrations are serial. Linux amd64 CPU
execution is translated. macOS CLI/MCP, seven CTests and both protected-text
scans pass. Unix owning root e2fc5b48 differs from compiled root dff8414a only
in two engineering records and the gitlink; actual source/config bytes agree.
Windows verifies16732 source files/123 patches and the unchanged374 B offline
adapter. Actual static-CRT Assist/core archives1108970370 B/722136118 B pass
4673/260-member walks with41 source/two helper identities and empty core
features. Finalized evidence/prose is separate from the compile-time snapshot.
Live IDA, Qt harness, packages and physical x64 Windows are not established.

The identical11177 B observer advances42609->42802, exactly193 scheduler calls,
to service0x1B1/PC0x180002B50, thread8, with
[0x20,0x3,0xABE8F0,0x4]. The new explicit failure is class3/
WorkerFactoryBindingCount. All four ordinary smoke/MSVCRT/UCRT/installed-whoami
programs still report reason5/exit field0/internal failure before guest exit.
Complete worker-factory behavior and process execution remain incomplete.

Run `python3 check_native_worker_soft_maximum.py` in this directory. Historical
validation requires the source checkout matching all41 frozen source hashes.
The checker binds original producer/log/query/state identities, the observed red
baseline, nonempty targeted/broad/owning results and the unchanged observer.
Six controlled negatives cover omitted manifests, corrupt raw bytes, rehashed
invalid native statuses, empty filters, forged summaries and optimized Python.
