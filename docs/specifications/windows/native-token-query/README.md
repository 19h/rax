# Captured primary-token AppContainer query

The installed ARM64 loader reaches `NtQueryInformationToken` class 29 with
current-process-token pseudo-handle `-4` after the cookie-ownership correction.
This group implements `TokenIsAppContainer` from the primary subject already
captured for the native WNF catalog. It does not query the host token during a
guest syscall or invent a false property when the subject is absent.

`Catalog::selected` retains the captured Boolean even when the catalog has no
records. A default catalog has no captured subject. The native query recognizes
process/effective token pseudo-handles and guest-owned primary-token objects;
access to a real guest token requires `TOKEN_QUERY` (`0x0008`). No host handle
is stored in that object. General token creation, impersonation and other token
information classes remain explicitly unsupported.

| Handle role | Result after caller probes |
| --- | --- |
| Current process token `-4` | Captured primary subject |
| Current thread token `-5` | `STATUS_NO_TOKEN` in the existing no-impersonation model |
| Effective token `-6` | Captured primary subject |
| Real guest primary token with query access | Captured primary subject |
| Real guest primary token without query access | `STATUS_ACCESS_DENIED` |
| Process, thread or other object type | `STATUS_OBJECT_TYPE_MISMATCH` |
| Null or invalid handle | `STATUS_INVALID_HANDLE` |
| Opaque token without a modeled subject | Explicit unsupported result |

For the measured class 29 contract, a nonempty output must be aligned to four
bytes and its complete supplied extent is probed before handle validation.
The four-byte `ReturnLength` is mandatory and permits unaligned stores. Both
probes precede handle/type/access checks. Read-only guarded pages consume the
visited guard before write-access failure. A valid query writes return length 4
before checking the minimum output length; on success it then writes one DWORD,
so overlapping output wins. Remaining output bytes are unchanged.

```text
admit class 29
if length != 0: check output alignment 4; probe output[0:length]
probe ReturnLength[0:4]
resolve guest token; check type/access and retained subject
store ReturnLength = 4
if length < 4: return STATUS_BUFFER_TOO_SMALL
store output DWORD = captured Boolean
return STATUS_SUCCESS
```

The adapter uses constant auxiliary storage and performs no allocation or
entropy advance. For `p` visited pages, it makes `O(p)` calls to the existing
address-space translator; the first inaccessible page ends the probe. It does
not allocate a buffer proportional to the caller-supplied length.

The native producer executes 1,830 calls per profile on ARM64, x64, x86 and
x86-LAA: 7,320 calls on one Windows 10.0.29683 ARM64 kernel. Ten handle roles are
combined first with eleven lengths, then with 43 pointer/protection roles and
four lengths. Forty repeated baseline controls per profile are intentional.
Four separately read byte cells per destination preserve readable prefixes at
page crossings. Protection snapshots precede readback, and every row preserves
the sentinel Win32 last-error value. All four producers close their three
handles and release their arena; no structured exception escapes a query.

`check.py` binds the actual producer/PE identities, raw stdout and ordered
matrix to `cases.json`. It recomputes the normalized semantic rows for every
profile. The kernel observations are separate from guest replay and installed
leaf validation. No native executable or library is committed.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| T1 | The native process retains the primary subject used for WNF. | Existing `wnf::snapshot` captures class 29 with exact DWORD validation. | Query from captured state, without a new host call. | Empty catalog, cloning, false/true/missing subject. | An admitted native process reaches the query without the captured property. | Confirmed by empty/clone/true/false tests and installed ARM64/WoW64 selection; other native subjects remain unmeasured. |
| T2 | Effective token means primary token in the current no-impersonation model. | Native pseudo-handle controls and existing thread-token contract. | Same property for `-4` and `-6`; no thread token for `-5`. | Invalid/type/access handles, guards, aliases and full supplied spans. | A supported impersonation path or native control contradicts it. | Confirmed for controlled no-impersonation subjects; general impersonation unknown. |
| T3 | A captured true Boolean serializes to DWORD 1. | Microsoft class 29 contract; existing nonzero-to-Boolean capture. | True-state guest serialization. | True/false retained-state tests and suffix preservation. | A malformed capture is accepted or serialization changes adjacent bytes. | Native true-token measurement unknown; source/test proof is separate. |
| T4 | Measured probe order is shared by the four selected profiles. | Four native producer matrices and independent PE inspection. | One shared adapter policy. | 7,320 native rows plus real installed stubs on available host profiles. | A raw row or installed-leaf replay disagrees. | Confirmed by all four portable replay profiles and actual installed ARM64, x86 and x86-LAA service leaves on this host. |

The affected RAX planes are native service dispatch/export registration, the
internal guest object type, primary-subject retention and tests. Object waiting
explicitly treats primary tokens as nonwaitable. Loader initialization, native
snapshot acquisition, registry/WNF access decisions, CPU decoding/SMIR/JIT,
filesystem, transport, persistence and public C ABI layouts are unchanged.
Windows, macOS and Linux compile the same adapter and portable replay tests;
actual installed Windows service leaves require a Windows host. RAX 1.11.0 and
empty core features remain the embedding contract.

Assist plugin/UI/lifecycle, conversations, providers, prompts, tool schemas,
approval policy, main-thread dispatch, Mesh/MCP transport and persisted state
have no code changes. The RAX pin, engineering records and scoped embedding
checks are the affected Assist surfaces.

Bounded findings:

- High: full ordinary Windows process execution remains unproven. This one
  information class does not establish general token, native I/O or all loader
  services, and cannot by itself complete the process-userland objective.
- Medium: all native producer profiles ran on the same ARM64 kernel. Physical
  x64 kernels, other Windows builds and a native AppContainer subject are not
  measured by these records.
- Medium: x64 guest installed-runtime acquisition on this ARM64 host remains a
  separate limitation. Portable replay covers x64 semantics; actual installed
  leaf checks must identify the profiles they really execute.

Primary contract sources are the pinned PHNT `ntseapi.h` snapshot and license in
`primary/`, Microsoft's [TOKEN_INFORMATION_CLASS definition](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-token_information_class)
and the [NtQueryInformationToken DDI reference](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntqueryinformationtoken). Native measurements own the observed probe
ordering and failure precedence; public API prose does not establish those
private details.

## Validation and continuation boundary

The compiled source has two retained epochs. The first Windows focused-test
build exposed an incorrect module qualifier in a Windows-only test helper.
The second epoch changes only that qualifier. The prefix containing all Unix
compiled items is byte-identical; every other selected input is identical.
Both original failure output and both source epochs are retained. The final
source passes fresh Unix compile/focused checks and three-host owning builds
against the second epoch.

The native installed-leaf test executes every one of the 1,830 rows through
actual selected ARM64, x86 and x86-LAA stubs: 5,490 installed-leaf calls. The x64
profile is covered by portable replay and the native producer. It is not an
installed x64 guest-runtime claim on this ARM64 host.

The owning checks use the retained isolated Assist carrier. Its public ABI
minor is 184; current Assist is 186; RAX remains 1.11.0. The selected archives use
empty core features and bind 313 source inputs. These are scoped C API/C++
embedding checks, not a whole-current-Assist or interactive IDA validation.

The unchanged observer now reaches `NtAccessCheck` service 0 at
`0x180001000`: turn2,135,461 for ARM64 smoke and2,137,823 for ARM64 whoami.
Neither observation is a successful guest exit. A third absent fixture is
explicitly excluded. The driver exit status records control completion only.

`validation-check.py` binds source epochs, completed commands, actual counts,
the exact retained Windows lowerer failures, and complete owning archives.
No failed preliminary run is recast as a passing final gate.


| Host | Full library passed/failed/ignored | All-targets | C API | Integration |
| --- | --- | --- | --- | --- |
| macOS | 7842/0/2 | Passed | 168 | Windows 544 |
| Linux | 7836/0/2 | Passed | 168 | Windows 544 |
| Windows ARM64 | 7267/5/2 | Passed | 168 | Native memory 4 |

The five Windows failures are the existing four BZHI and one FP16 lowerer
failures. No test is filtered from these full-library results. Two existing
ignored tests remain ignored; focused tests and compiled products are listed
separately in the retained command records.


| Scoped embedding surface | macOS | Linux | Windows ARM64 |
| --- | --- | --- | --- |
| Completed owning command gates | 6 | 4 | Shipping Rust plus five C++ consumers |
| Selected CTests / scans | 7 / 2 | Five C++ consumers | Five C++ consumers |
| Complete selected archive walks | 2 | 2 | 2 |
| Bound source inputs / core features | 313 / empty | 313 / empty | 313 / empty |

The final native driver completes both core and embedding phases with status0.
The complete evidence checker passes. Five controlled alterations are rejected:
raw corruption, empty native gates, forged zero native failures even with
matching updated hashes, incomplete embedding, and missing final Unix gates.
These controls are retained without changing original successful/failed logs.
