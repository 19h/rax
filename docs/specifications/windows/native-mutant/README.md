# Native guest mutants and installed named-object identity

`NtCreateMutant`, `NtOpenMutant`, `NtReleaseMutant` and `NtQueryMutant` classes
0/1 now use private guest mutex objects. Guest recursion, ownership, abandonment,
duplicate/reference lifetime and existing wait helpers share that state. No guest
request opens, acquires or mutates a host synchronization object. Native wait
services remain unimplemented; wait coverage here exercises existing guest wait
helpers and Win32 consumers, not an implemented `NtWaitForSingleObject` entry.

Installed Win32 event/mutex/semaphore calls and native mutants share selected
directory identity and a counted UTF-16 leaf. Embedded-mode name keys and error
precedence remain unchanged. Counted NT names preserve embedded NUL and unpaired
surrogates; Win32 names terminate at NUL. Native and Win32 independent object
comparisons distinguish the truncated identity. Explicit insensitive lookup
selects the newest matching guest object; retained native sequences include
intervening exact-case opens. This is a bounded observed policy, not a claim
about undocumented kernel indexing internals.

The selected one-principal profile implements measured generic/MAXIMUM_ALLOWED
mapping, zero grants, low-byte BOOLEAN, OPENIF/collision, root/type checks and
final-close removal. Query classes retain exact lengths, untouched padding,
optional ReturnLength, native probe ordering and WoW64 CLIENT_ID conversion.
Release accepts any mutant grant and probes PreviousCount before object lookup.
Explicit security descriptors, QoS, permanent lifetime and ACCESS_SYSTEM_SECURITY
remain explicit unsupported cases. In particular, the native host can grant
ACCESS_SYSTEM_SECURITY; this implementation does not invent privilege denial.

Private BNO prefixes and missing installed namespace metadata refuse named Win32
calls with ERROR_NOT_SUPPORTED. Anonymous calls remain available. Retained
prerequisite observations show private Local/plain routing and public Global
routing, so the implementation does not substitute a public default directory.
Private BNO subtree capture is still required for those contexts.

| Retained native matrix | Cases per ABI profile | Four-profile total |
| --- | --- | --- |
| Grant/create/lifecycle revision 2 | 223 | 892 |
| Query faults, exact copies, padding and guards | 6300 | 25200 |
| Release faults and before/after state | 50 | 200 |
| Create faults and aliased publication | 3200 | 12800 |
| Naming revision 4 | 144 | 576 |
| Naming revision 5 | 156 | 624 |
| Naming revision 6 | 156 | 624 |
| Win32 versus NT object comparisons in revision 6 | 12 | 48 |

All four profiles run on one Windows10.0.29683 ARM64 kernel: ARM64, compatibility
x64, x86 and x86 with large-address awareness. This does not establish another
physical kernel architecture or principal's grants. Producer source, PE and both
NTDLL identities match before/after; all acquired probe handles close. The Win32
comparison API is loaded in the actual process, but Win32 DLL image identities
are not comprehensively pinned. Native bytes own the twelve replay projections.
Earlier unsuccessful producer/build attempts remain retained and are not counted
as successful observations. `check-native.py` validates those boundaries.

| Host | Full core passed/failed/ignored | Focus | C API | Integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7945/0/2 | 11 | 168 | 544; native-memory cfg excluded |
| Linux x86-64 container | 7939/0/2 | 11 | 168 | 544; native-memory cfg excluded |
| Windows ARM64 | 7384/5/2 | 11 | 168 | 4 native-memory; Unix corpus cfg excluded |

The exact five existing Windows lowerer failures remain; both ignored microkernel
tests remain explicit. All-target checks pass on all three hosts. The tests-only
published-parent baseline compiles and fails all eleven focused tests. Prototype
epoch1 exposed two WoW64 fault-order cases; both were corrected before the final
epoch4 three-host runs. Final selected inputs bind5762 source/fixture files.
That list is not a complete compilation-input closure: the native preparation
initially omitted unchanged package metadata and static include fixtures. The
failed builds are retained. Final Windows compilation restores23 package files
and13236 baseline files, verified around each gate. Nine historical documents
and sixteen Git symlink representations are explicitly separated; no selected
compiled override is replaced. The native Cargo source-only configuration is
also bound. See validation/mutant-native-restored-inputs-review.json.

Retained Assist carriers pass seven macOS CTests and protected executable scans,
and five C++ consumers on Linux and Windows. Two actual Cargo-selected archives
per host have complete structural member walks and empty core features;456
selected source/fixture inputs match. Frozen Rust ABI184 remains distinct from
the current root `c877fbe926b139f9a618ecf06f3d065ff510a2cc` / ABI263.
Eight affected RAX adapter/consumer C++ files match. The separately changed
static-text bridge and whole-current-root GUI/plugin/package execution are not
established by these retained-carrier results.

The observer links the native retained owner's actual core archive and executes
the unchanged ordinary startup prefix. Its source, executable, archive and both
targets have identical before/after identities:

- `terminal turn=2151176 Complete(Internal("native NTDLL service NtWaitForSingleObject (0x4) at 0x7fff38a41040 is not implemented"))`
- `terminal turn=2153525 Complete(Internal("native NTDLL service NtWaitForSingleObject (0x4) at 0x7fff38a41040 is not implemented"))`

Both unchanged ordinary programs create a guest mutant successfully and then stop at unimplemented NtWaitForSingleObject (0x4). Neither reaches a successful guest exit. Observer controller exit0 is not a guest exit.

Named lookup scans C guest objects and U UTF-16 units in O(C U) time with O(1)
scan storage; the stored name uses O(U) space. Parent parsing retains the existing
63-reparse and32766-unit bounds. Direct ISA, SMIR, native lowerers, JIT, public
C ABI, dependency pins, default features and package layouts are unchanged. The
bounded group does not complete same-host userland emulation.

Primary interfaces: [CreateMutexW](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexw),
[CompareObjectHandles](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-compareobjecthandles)
and retained PHNT declarations. Exact copies/hashes are listed in
primary-provenance.json. `check-private-bno.py` verifies the earlier prerequisite
observations; `check-validation.py` verifies final sources, gate selection,
archives and runtime boundaries. Assumptions, affected planes, limitations and
quality-gate evidence are recorded beside this file. Seven native and nine final
corruption controls reject inconsistent claims after outer digests are rebound.
