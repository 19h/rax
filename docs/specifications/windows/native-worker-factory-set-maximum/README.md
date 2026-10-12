# Installed NT worker-factory maximum setting

Measured on Windows 11 ARM64 10.0.29683.1000, 2026-10-12. Four native
ARM64/x64/x86/x86-LAA executables invoke installed NTDLL. Native principal:
SYSTEM/S-1-5-18. Compatibility execution is distinct from physical Windows
x64 execution. Actual producer source/image/log identities are retained.

`NtSetInformationWorkerFactory` class5/WorkerFactoryThreadMaximum captures a
four-byte ULONG, validates the typed factory handle and right0x4, and updates
the existing guest object's unsigned maximum. Exact length4 precedes input
capture, which precedes handle validation. Readonly, unaligned and last-page
inputs work; unmapped/inaccessible/cross-page inputs fail. A guard returns
STATUS_GUARD_PAGE_VIOLATION, consumes the guard once and leaves maximum intact.
Invalid length leaves the guard intact. Type errors precede access errors.
Zero and every captured high-bit value, including UINT32_MAX, succeed without
starting workers in the measured dormant factories.

```text
class5 -> exact length4 -> capture ULONG -> typed handle -> right0x4 -> maximum
             |                |              |               |
             v                v              v               v
         mismatch         memory fault   handle/type error  access denied
```

| Native rows per profile | Count |
|---|---:|
| Unsigned maximum values |11 |
| Input protection/alignment/page boundary and length |72 |
| Handle/type/access combined with input/length precedence |156 |
| Total per profile |239 |
| Total exact replays across four profiles |956 |

All native rows have zero external SEH, successful before/after query oracles,
zero TotalWorkerCount and unchanged handle counts. Query class7 requires exact
120 B/native64 or96 B/WoW64. The initial128 B query was rejected and is not
maximum/reference evidence. The corrected captures retain all128 B backing
bytes and check the untouched tail. Query is an oracle here; guest
NtQueryInformationWorkerFactory is not implemented by this change.

Guest state changes only after successful capture/type/access checks. Aliases
observe the same maximum; port/process references and constructor parameters
remain intact. Other information classes stop explicitly as unsupported.
Worker creation, activation, completion waits, active-factory resizing and
guest application exit remain separate incomplete contracts.

Lookup and update use O(log H) handle lookup and expected O(1) object access;
the input capture and additional space are O(1), exactly4 B. The object shape,
C API1.11.0, Assist Rust ABI184, defaults, toolchains, native admission and
permission/schema contracts do not change. The service has four arguments;
x86 caller cleanup is4*4 B+4 B return=20 B.

The private enum/prototype agrees with pinned PHNT commit
`53fbbdc5b5d2b08761db1c7b26bfa8c820924356`, whose header and license remain in
[constructor evidence](../native-worker-factory-create/README.md).
[Primary header](https://raw.githubusercontent.com/winsiderss/phnt/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntexapi.h).
Exact status/order/state behavior comes from the retained installed-kernel
measurements. Assumptions, source/consumer planes and bounded findings are in
`scope.md`. Final host results and the unchanged observer are bound by
`validation-summary.json` and `check_native_worker_maximum.py`.

Run `python3 check_native_worker_maximum.py` from this directory. For historical
validation, supply this evidence directory and a source checkout matching its
39 frozen source hashes. It rejects optimized Python, incomplete matrices,
empty filtered tests, identity mismatches and source/build divergence.

## Final validation

| Gate | macOS | Linux | Windows |
|---|---:|---:|---:|
| Focused tests passed |4 |4 |5 |
| Full library passed/failed/ignored |7642/0/2 |7634/2/2 |7042/5/2 |
| C API passed |168 |168 |168 |
| Registered guest integrations passed |544 |544 |4 |
| Owning C++ consumers passed |5 |5 |5 |
| All-target build |pass |pass |pass |

The unchanged Linux timeout-update/multishot cases pass isolated; full-suite
failures remain recorded. Windows retains four BZHI and one FP16 lowering
failure. No assertion or platform skip is weakened. Unix C API and Linux
integration execution are serial; Linux amd64 execution uses CPU translation.
Owning macOS CLI/MCP, seven CTests and both protected-text scans pass.
The Unix owning root is e2fc5b48; its source/config/resource bytes equal root
b760e682 except two engineering records and the gitlink. Windows verifies all
16618 files and100 patches in the applied build snapshot, separately retaining
the unchanged374 B offline adapter. Its static-CRT Assist/core archives contain
1108977222 B/722141702 B and pass4673/260-member walks. Thirty-nine source and
two helper identities bind the core/test/consumer snapshot; core features are
empty. Finalized evidence/prose is distinct from compile-time snapshot bytes.
Packages, live IDA and Qt execution are not established by these checks.

The byte-identical11177 B observer advances42564->42609, exactly45 scheduler
calls, to the same service0x1B1/PC0x180002B50 with class14 and arguments
[0x20,0xE,0xABE730,0x4]. The explicit failure is
NtSetInformationWorkerFactory class14/WorkerFactoryThreadSoftMaximum. All four
ordinary smoke/MSVCRT/UCRT/installed-whoami programs still report reason5,
exit field0 and an internal failure before guest exit. Maximum setting is
proved; complete worker-factory behavior and process execution remain incomplete.
