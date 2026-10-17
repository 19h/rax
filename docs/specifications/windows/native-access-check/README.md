# Captured-subject native access checks

The installed ARM64 smoke and whoami processes reach `NtAccessCheck` after the
captured AppContainer query. Their pre-call inputs are an absolute descriptor
with control `0x0004`, matching owner/group SIDs, one allow ACE with mask 1,
generic mapping `0x20001/0x20000/0x20000/0x1f0001`, desired access 1, current
process token `-4`, and a 56-byte privilege buffer. The controlled native
equivalent denies access through the separate `AccessStatus` output while
returning successful function status. A blanket success or denial would lose
that distinction.

The descriptor's ACL has 236 bytes of capacity: an 8-byte header, one 52-byte
ACE, and 176 unused bytes. The SID has 9 subauthorities: `8 + 4*9 = 44` bytes.
The access-check parser accepts that unused ACL capacity without changing the
existing, narrower WNF descriptor parser.

## Implementation boundary

Native runtime selection copies the primary user, groups, restricted SIDs,
AppContainer flag and privileges. Two consecutive captures must agree. Guest
calls use the retained immutable subject and never pass a guest handle, SID,
descriptor, pointer or desired mask to a host token API. Captured impersonation
objects hold their own immutable subject; creating or attaching arbitrary
thread impersonation tokens remains a separate service boundary.

The common evaluator handles ordered classic allow/deny ACEs, enabled and
deny-only groups, implicit owner rights, absent/NULL versus empty DACLs,
`MAXIMUM_ALLOWED`, and measured `SeSecurityPrivilege`/`SeTakeOwnershipPrivilege`
use. It refuses restricted/AppContainer subjects, callback/object ACEs,
`OWNER RIGHTS` ACEs, unmeasured descriptor controls/SACLs and malformed records
outside its admitted profile. A refusal is an explicit diagnostic rather than
a fabricated access result. These limits do not complete general Windows
security or the full same-host process objective.

The guest adapter separates native64 capture from WoW64 header conversion.
Native64 resolves a token before capturing its descriptor. The measured WoW64
wrapper reads a non-NULL descriptor header earlier, so descriptor faults can
precede token errors. Descriptor alignment is 4 bytes; a separate 16-offset
ARM64 control confirms that offsets 0, 4, 8 and 12 succeed.

After reading the caller's capacity, a non-NULL privilege buffer with at least
20 bytes receives the early zero privilege count. Mapping input is then read
and generic desired-access bits are rejected before token validation. Valid
descriptor and privilege policy determine the required capacity. A missing
security privilege reports `STATUS_PRIVILEGE_NOT_HELD` through `AccessStatus`
before insufficient-buffer reporting. Otherwise the required capacity is
`max(20, 8 + 12*n)` bytes for `n` used privileges. The actual published header
is only `8 + 12*n` bytes. Successful calls preserve the supplied capacity.
Final `AccessStatus` precedes `GrantedAccess`, including aliases.

Input capture uses bounded SID and ACL copies. For `A` admitted ACEs and `S`
captured SIDs, evaluation takes `O(A*S)` time and `O(A)` auxiliary space;
`A <= 4096`, SID extent is at most 68 bytes, and ACL extent is a 16-bit byte
count. The caller's privilege capacity does not cause proportional allocation.

## Native evidence

The logical producer makes 4,410 calls per profile on ARM64, x64, x86 and
x86-LAA: 17,640 calls on one Windows 10.0.29683 ARM64 kernel. The pointer
producer makes 1,500 calls per profile: 6,000 calls. Logical rows agree across
all four profiles. Pointer rows agree within native64 and within WoW64, with
108 differences between those groups. Every row retains last error, output
bytes and protection state; every producer reports owned-resource cleanup.

Private-token probes use independently duplicated impersonation tokens. They
adjust those objects only; no thread adopts them and the original primary
token is query/duplicate-only. The second probe records 1,296 ARM64 calls with
four privilege configurations, four descriptors, nine masks and nine buffer
capacities. It records privilege order 9 then 8 when both are used, control 0,
and `SE_PRIVILEGE_USED_FOR_ACCESS` (`0x80000000`). The earlier 972-call probe is
retained. Its historical "unowned" labels designate World-owned descriptors;
World is an enabled group and the second probe names it explicitly, alongside
the verified nonmember SID.

The three added NTSTATUS-to-Win32 mappings are independently measured as
`0xC000005C -> 1309`, `0xC0000079 -> 1338`, and `0xC00000E6 -> 1360`.
`check.py` verifies the complete Cartesian matrices, recorded producer/source
identities, exact byte fixtures, cleanup, alignment, translations and the
ordinary pre-call trace. Native observations remain separate from emulated
replay and application completion.

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | The selected primary subject remains immutable for guest access decisions. | Existing bounded native subject acquisition; added privilege capture; no token-adjustment guest service. | Pure access evaluation and primary pseudo-token behavior. | Empty catalog, different captured principals, missing capture, independent impersonation subject. | A supported guest service mutates that token without updating its captured state. | Retained within the current no-attachment model. |
| A2 | The measured native64 and WoW64 capture orders are distinct. | Four complete pointer matrices with 108 cross-group differences. | Fault, guard and handle-error priority. | All 6,000 pointer rows, aliases, crossings and `0xffffffff` capacity. | An installed leaf replay differs from its native profile. | Confirmed by final portable and installed ARM64/x86/x86-LAA replay. |
| A3 | Ordered classic DACL and two-privilege evaluation covers the admitted descriptor profile. | Logical matrix, private-token controls and captured continuation input. | Granted mask, access status and privilege header. | Deny/allow order, empty/NULL DACL, owner and nonmember SIDs, missing/enabled privileges, short buffers. | A native row disagrees or an admitted unsupported ACE receives a result. | Confirmed for the admitted profile by final four-ABI portable replay and native leaves; other ACE/token profiles remain explicitly unsupported. |
| A4 | Native evidence is specific to the measured Windows kernel and subjects. | Every producer records build, architecture, identity and cleanup. | Scope of claims across hosts. | Independently inspect all four producer PEs; preserve physical-host and subject boundaries. | A different kernel or subject is presented as independently measured by this corpus. | Confirmed provenance limit; physical x64 and other Windows builds are unmeasured. |

## Source and target surfaces

Affected RAX planes are captured-subject ownership, native service registration,
guest token objects, AppContainer queries on those objects, status translation,
and shared/native replay tests. WNF permission policy remains unchanged. No
CPU decoder, SMIR, lowerer, JIT, filesystem, transport, persistence, public C
ABI, dependency pin or default feature changes.

Windows performs bounded native acquisition. Windows, macOS and Linux compile
the same guest adapter and evaluator; the existing unavailable-host path for
selecting Windows DLLs on another operating system remains explicit. Final
three-host compilation and complete library suites have finished. Scoped
Assist embedding is retained as a separate validation surface below.

The initial synthetic x64 fixture exposed a caller-storage error: its stack
pointer was `0x20ffc0`, while the descriptor allocation began at `0x210000`.
Argument 7 was stored at `SP + 8 + 7*8 = 0x210000`, overwriting the descriptor.
The corrected fixture reserves 256 bytes within the caller stack. Original
failures and debugger observations are retained as a separate source epoch;
production adapter code did not change for that correction.

Bounded findings: **High**, full ordinary Windows application completion and
general security semantics remain unproven. **Medium**, privileged output
fault conversion on WoW64, physical x64 kernels, restricted/AppContainer
subjects and other Windows builds are not established by the ARM64 private
token probe. **Medium**, installed x64 runtime selection on an ARM64 host is a
separate existing limitation. These findings limit completion claims and are
not silently converted into platform exceptions.

Primary definitions are retained under `primary/`, including the pinned PHNT
declaration and Microsoft's AccessCheck/GENERIC_MAPPING documentation. Native
measurements define private NT failure ordering; the documented Win32 BOOL
wrapper is a different return contract.

## Final core validation

| Host | Full library passed/failed/ignored | Access-check focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7848/0/2 | 6 | 168 | 544 shared cases |
| Linux x86-64 container | 7842/0/2 | 6 | 168 | 544 shared cases |
| Windows ARM64 native | 7274/5/2 | 7 | 168 | 4 native memory cases |

Every complete library selection has zero filtered tests. All-target checks
pass on all three hosts. The native focus includes actual installed ARM64,
x86 and x86-LAA calls; an installed x64 runtime on this ARM64 host remains
unverified. The five Windows failures are the same four existing AArch64 BZHI
lowerer tests and one FP16 lowerer test. The two ignored microkernel tests
remain explicit. Windows `user_windows` is cfg-empty; its compilation is not
counted as runtime integration coverage.

The unchanged ordinary observer reaches `NtQueryWnfStateData`, service
`0x170`, PC `0x180002740`, at turns 2,139,682 and 2,142,044 for ARM64 smoke and
whoami respectively. Each advances 4,221 observer turns from its preceding
access-check stop. These are continuation observations, not successful
application exits. The missing third fixture is explicitly excluded.

## Scoped Assist embedding

The final carriers pass on all three hosts. macOS runs seven CTests and two
protected-string scans; Linux and Windows each run the five selected C++
consumers. Each host's artifact stream binds 319 selected RAX source/fixture
inputs to an empty-feature core and two complete archive member walks.
Nine selected current-root C++ files match the retained carrier sources.

The frozen Assist public ABI minor is 184; the compared current root is 186.
RAX C ABI remains 1.11.0. These results establish the scoped embedding, not a
whole-current-root, plugin, interactive IDA or package build. Source epochs,
initial failures, exact commands, raw logs and artifact identities are under
`validation/`; run `python3 -B validation-check.py` to verify them.

The evidence checker also rejects five controlled alterations: corrupt raw
output, an empty native gate list, a forged zero-failure Windows summary, an
incomplete owning driver and missing final Unix gates. The stress driver and
its results are retained with the other source-bound evidence.
