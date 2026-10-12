# Installed wait completion packet construction (2026-10-12)

`NtCreateWaitCompletionPacket(PHANDLE, ACCESS_MASK, PCOBJECT_ATTRIBUTES)` creates a
real process-local `WaitCompletionPacket`, with shared signal state and per-handle
access/inheritance. A new packet is notification-signaled: repeated waits do not
consume its signal. Duplicate handles retain the object until final close.
Association, cancellation and completion delivery remain separate services.

The pinned PHNT `ntioapi.h` declares the three arguments. The independent
installed-kernel programs establish validation order and state, rather than
deriving expected results from the emulator. Four caller profiles (ARM64, x64,
x86 and x86 with large-address awareness) supply:

| Family | Rows per profile | Total |
|---|---:|---:|
| Constructor, access, flags, faults, guards, alignment and aliases | 317 | 1268 |
| Repeated waits, duplicate aliases and final close | 18 | 72 |
| Dormant legacy/IR timers and automatic/manual event aliases | 7 | 28 |
| All families | 342 | 1368 |

Every original constructor row records the complete accessible output, backing
and attribute byte extents. Replays compare 1228 rows exactly, including guard
consumption and resource counts. Forty rows requiring NT namespace, security
descriptor, QoS or privileged grants are explicitly excluded; targeted tests
require an unsupported diagnostic and no publication at those boundaries.
Only the newly allocated native HANDLE span is relocated in replay comparisons.
No captured padding, alias or suffix is discarded.

All successful native handles identify as `WaitCompletionPacket`.
`NtQueryIoCompletion` returns `STATUS_OBJECT_TYPE_MISMATCH` (0xC0000024), rather
than treating the packet as a completion port. Every original row closes its
handles and records equal process handle counts before and after.

| Access input | Effective grant |
|---|---|
| Explicit supported bits | input AND 0x001F0001 |
| GENERIC_READ, GENERIC_WRITE or GENERIC_EXECUTE | 0x00020001 |
| GENERIC_ALL or MAXIMUM_ALLOWED | 0x000F0001 |
| Explicit SYNCHRONIZE | 0x00100000, accepted |
| ACCESS_SYSTEM_SECURITY | Native caller privilege dependent; explicit unsupported guest policy |

Generic mappings exclude `SYNCHRONIZE`. Without that right, native NT waits
return `STATUS_ACCESS_DENIED` (0xC0000022); public waits return `WAIT_FAILED`
(0xFFFFFFFF) and error 5. With it, single and repeated waits return zero.
Wait-any accepts duplicate aliases and returns the first signaled index;
wait-all rejects aliases with error 87 before consuming state. The event
comparisons independently show that wait-any consumes an automatic event once.
Dormant legacy and extended timers return `WAIT_TIMEOUT` (0x102) through public
waits. The shared public wait path now admits both timer variants and packets.
Blocked wait-any aliases retain each reference and release each exactly once.

Native64 probes the full unaligned HANDLE output before capturing aligned
48-byte OBJECT_ATTRIBUTES. WoW64 converts the 24-byte structure before probing
the 4-byte output. Length, cross-page reads, protected destinations and aliased
publication are covered by the original rows. Object-manager flags outside
0x1EF2 fail with `STATUS_INVALID_PARAMETER` (0xC000000D). A nonzero unnamed root
then fails with `STATUS_OBJECT_NAME_INVALID` (0xC0000033), before the packet
type's `OBJ_PERMANENT` rejection. With root zero, permanent creation fails with
`STATUS_INVALID_PARAMETER`. Successful inheritance follows attribute bit 2.
WoW validation failures copy a zero HANDLE after attribute conversion and output
probing; faults and unsupported policy do not reach that copy.

The installed kernel is ARM64 Windows 11 build 29683.1000. x64/x86 caller
profiles are compatibility processes on that kernel, not physical x64 Windows
proof. Twelve actual source/executable identities, installed DLL identities,
and the caller principal/privilege observations accompany the raw captures.
Different kernels, caller SIDs, named objects, token/ACL authorization and
cross-process object namespaces remain unverified.

Constructor work uses fixed-size probes and O(1) policy/state work. Existing
handle allocation costs O(H) time and O(1) extra space for H open handles.
Wait sets use expected O(N) time and O(U) extra space for N identifiers and U
distinct objects, with N <= 64 and at most 64 * 8 = 512 bytes of input handles.
Public Rust object variants change additively; the C ABI remains unchanged.
Known in-tree consumers and all owning shipping surfaces require compilation.

## Validation status and assumptions

The semantic red baseline compiled and failed two constructor tests because
the service was absent. Seven portable tests cover construction, exact replay,
grants, unsupported boundaries, public waits and duplicate/pin/final-close
lifetimes. Installed-leaf tests additionally validate native ARM64/WoW64 return
and three-argument cleanup. Final gate records below preserve passes, failures and characterization.

Assumptions and falsification probes are maintained in `task-contract.md`.
The process-local model has no host kernel packet or implicit privilege grant.
High full-goal work remains: association, cancellation, completion delivery,
other native startup services, and complete ordinary program execution.
Medium limits include physical x64 Windows, alternate caller/build profiles,
native host runtime timing failures and package/live IDA/Qt validation.

The final gates pass seven portable tests on macOS/Linux and eight Windows tests,
168 C API tests on each host (Linux serial characterization), all-target builds,
544 Unix-side Windows integration tests and four native memory tests. Fresh root
e2fc5b48 CLI/MCP, seven CTests and both protected-text scans pass. All five owning
C++ consumers pass on each host; Linux reuses the recorded prior generated
static-text/Monocypher/JSON inputs. No package, live IDA or Qt-harness run is claimed.

| Full library run | Passed | Failed | Ignored |
|---|---:|---:|---:|
| macOS | 7628 | 1 | 2 |
| Linux | 7621 | 2 | 2 |
| Windows | 7027 | 5 | 2 |

The macOS native `/usr/bin/true` test exceeded its 30 s wall limit in the parallel
full run and passed separately in 24.4 s. The two Linux io_uring timeout tests
failed in the full run. Linux's initial parallel C API run passed 167 and failed
one because a 1 s run returned RAX_PROCESS_TIMEOUT (6) instead of expected
RAX_PROCESS_FAILED (5); the isolated case and all 168 serial C API tests pass.
Windows retains four BZHI and one FP16 lowering failures. Original failures are
retained; root causes of the timing variation remain unknown. No failed full
run is represented as green.

The byte-identical 11177-byte observer now returns NTSTATUS zero for the packet
constructor at call 42172 and stops at 42185 for NtAssociateWaitCompletionPacket
service 0x92 / PC 0x180001960, with eight arguments
[0x28,0x1c,0x24,0x1103320,0x11032f0,0,1,0xabe640].
42185 - 42172 = 13 scheduler calls, not instructions. Smoke/MSVCRT/UCRT/installed
whoami retain reason 5 / exit_code 0 with that internal diagnostic; guest exit
and full Windows process execution remain unproven.

    42172 constructor -> NTSTATUS 0 -> 42185 association stop

The native static-CRT Assist/core archives are 1115511964 B / 728841208 B, with
4673 / 260 complete member walks, 32 source identities, two helper identities
and empty core features verified. The native snapshot verified 16313 files and
162 patches; the unchanged 374-byte offline Cargo adapter is recorded separately.
Later evidence and prose updates are distinguished from compile-time overlay
bytes. Existing vm-memory ARM64 stdcall and SDK deployment-target warnings are
retained as medium compatibility findings.
