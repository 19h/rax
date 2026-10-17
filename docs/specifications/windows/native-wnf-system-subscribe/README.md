# Selected System WNF read subscriptions

The query-enabled smoke and whoami continuations stop on a System-scoped
`NtSubscribeWnfStateChange`: name `0x0280032EA3BC0875`, change stamp 1 and event
mask `0x11`. The earlier Process-scoped subscription is already supported.
This group extends the selected immutable environment to System read
subscriptions. It does not create a host subscription from a guest request.

The System descriptor has control `0x8014`, a 28-byte untrusted mandatory-label
SACL and a 344-byte DACL containing eleven generic-read allow ACEs. Its query
record supplies the initial change stamp. A separate bounded evaluator handles
classic allow/deny ACEs and measured generic read/write/all mapping. Unknown
controls, inherited/object/callback ACEs, OWNER RIGHTS, other mandatory labels,
generic execute, restricted subjects and AppContainers remain unavailable.
The narrower Process descriptor parser is retained.

Native subscription access requires `(mask & 0x11 ? 1 : 0) | (mask & 0x0e ? 2 : 0)`.
Four producer ABIs on Windows ARM64 build 10.0.29683 record 3072 permission
calls. Four ordered allow/deny descriptors add 384 ARM64 calls. All completed
producers retain executable/source identities and resource cleanup. An earlier
ordered-deny descriptor denied explicit deletion; a separate read-only query
after its creator exited confirms the temporary name no longer exists. The
corrected producer grants DELETE independently and deletes all four names.
Original failed builds and cleanup results remain preserved.

Fresh native children are stopped before their first notification-event
registration. Captured System data, owned empty data and owned populated data
cover 576 cases. A zero-length initialized state adds 192 cases. Read-mask/stamp transitions add 288 cases; three-subscription
sequences with manual resets add another 288. All 1344 measured cases and six
warmups retain child exit and stable parent handle counts. Existing System data
is unchanged; probe-owned names and descriptors are cleaned up.

A mismatching selected stamp can queue a data notification. Event registration
does not retroactively signal an existing queue. Matching a later requested
stamp does not discard queued data. Duplicate queued data does not resignal a
manually reset event. Dropping data interest removes that queued bit. The
existing Process subscription level/mask semantics remain separately tested.

The shared guest implementation supports read masks `0`, `1`, `0x10` and `0x11`.
Shared System subscriber-level notification state is not inferred from private
guest subscribers: an otherwise authorized request for bits `0x0e` refuses
explicitly. Access denial is decided first. Absent, denied and unstable query
snapshots remain distinct; missing data acquisition is not replaced with a
successful empty state. Query selection and event delivery remain separate.

Selection retains the existing maximum of 4096 metadata values. For `A` ACEs
and `S` captured SIDs, descriptor evaluation costs `O(A*S)` time and `O(A)`
scratch, with at most 4096 ACEs per admitted descriptor. Guest subscription
lookup/update costs `O(log N)` for `N` private subscriptions and constant extra
storage per record. No host pointer, guest handle or guest capacity reaches a
native API in this change.

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress and falsification probe | Status |
| --- | --- | --- | --- | --- | --- |
| S1 | The measured System descriptor profile can be evaluated against the captured primary subject. | 3456 native permission calls and the actual selected descriptor. | Read admission. | Ordered deny/allow, unknown labels/controls, OWNER RIGHTS, restricted/AppContainer subjects. | Native profile and portable adapter checks pass. |
| S2 | Selected change stamp and private queue state reproduce initial read notifications. | 1344 native fresh/transition cases. | Event signaling and retained data bit. | Registration order, matching/mismatching stamps, manual reset, removed/readded interest and third calls. | Native profile, four-ABI replay and installed leaves pass. |
| S3 | Shared System subscriber levels equal the private guest subscriber set. | No supporting evidence; other native subscribers may exist. | Subscriber-level events. | Capture native shared subscriber information before extending masks. | Rejected; authorized subscriber-level requests remain explicitly unsupported. |
| S4 | A successful nonzero selected change stamp identifies initialized data independently of payload length. | Empty initial state stamp 0; updated state stamp 1; stamp-driven native subscription contract. | Empty updated payloads. | Update a probe-owned state to zero-length data, then replay mismatch and matching subscriptions in a fresh child. | Confirmed by 192 fresh-child zero-length update cases; live updates remain outside this immutable selection. |

## Scope and validation

Affected planes are System descriptor admission, private subscription queues,
service refusal diagnostics and tests. Native acquisition is unchanged. Public
ABI, dependencies, default features, CPU lowering, persistence, transport, UI
and package layout are unaffected. The shared code compiles on Windows, macOS
and Linux; installed Windows metadata remains a Windows host capability.

`check.py` independently checks raw producer identities, exact fixtures, failed
attempts and cleanup. Cross-platform source and executable validation is retained below.
Full ordinary Windows application completion remains unproven. Shared System
subscriber-level state and notification delivery are high-impact remaining
boundaries for the full process goal; they are not successful stub paths.


The initial three-host tests compiled but exposed one fixture-ordering error:
when event registration preceded subscription, the test compared the later
subscription state against the native observation taken immediately after
registration. Epoch 2 moves that assertion to the matching observation point
and adds the zero-length initialized-data controls. Production implementation
bytes are unchanged between these epochs. Original results and source copies
remain retained; complete validation is recorded below.

## Completed validation

| Host | Full library passed/failed/ignored | C API | Scoped embedding |
| --- | --- | --- | --- |
| macOS ARM64 | 7865/0/2 | 168 | 7 CTests, 2 scans, 2 complete archives |
| Linux x86-64 container | 7859/0/2 | 168 | 5 C++ consumers, 2 complete archives |
| Windows ARM64 | 7295/5/2 | 168 | 5 C++ consumers, 2 complete archives |

All libraries ran unfiltered. Windows retains the same four BZHI and one FP16
lowerer failures, checked by exact name. Both ignored microkernel tests remain
explicit. All-target checks pass on each host. WNF focus/regression counts are
7/40 on each Unix host and 8/46 on Windows. Shared Windows integration runs
544 cases; native Windows memory integration runs 4.

Four guest profiles replay 3360 supported sequences and explicitly refuse 2016
unsupported shared-event requests. The installed ARM64, x86 and x86-LAA leaves
replay 2376 cases. Each scoped Assist carrier binds 332 source/fixture inputs,
empty core features and two complete archives. Frozen Assist ABI minor 184 is
distinct from current minor 186. Whole-current-root, GUI, plugin and package
validation are not established. Five altered/incomplete proof sets are rejected.

The final unchanged observer reaches turns 2144480 and 2146842 for smoke and
whoami, respectively: both advance 2596 turns past the previous subscription
boundary. The next raw trace is an unmapped instruction fetch at
`0x7ff82ff430c0`, followed by a cyclic SEH-frame diagnostic at
`0x7ff82b0dc4bc`. Both are unsuccessful application runs; the absent third
fixture is excluded. Native image-extension ownership is the next investigation.
