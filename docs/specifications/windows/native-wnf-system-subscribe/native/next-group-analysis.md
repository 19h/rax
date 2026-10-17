# System WNF subscription continuation: pre-implementation evidence

The final query build reaches two subscription calls in the smoke program.
The first, Process-scoped 0x41911A28A3BC10B5, is already supported. The failing
call is System-scoped 0x0280032EA3BC0875, change stamp 1, event mask 0x11.
The auxiliary trace and unchanged observer are separately identified; neither
is a successful guest exit.

The existing System descriptor has no owner/group, control 0x8014, a 28-byte
SACL with one mandatory-label ACE (type 17, no-write-up, S-1-16-0), and a
344-byte DACL containing eleven generic-read allow ACEs. Its metadata trailer
is maximum 8 plus a 16-byte TypeId. The old Process-only grant parser correctly
refuses this profile; broadening must preserve refusal of unknown descriptors.

Four actual producer ABIs ran 8 descriptor profiles * 3 stamps * 32 masks =
768 calls each, 3072 total. Required subscription rights are
(mask & 0x11 ? 1 : 0) | (mask & 0x0e ? 2 : 0). Generic read/write/all map to
1/2/3 for these native controls. All 32 temporary names were deleted, and all
1116 successful subscriptions were removed. Original failed compiler runs
remain evidence of attempts, not native behavior.

Fresh child controls cover 192 cases each for the selected read-only System
state and owned empty/value System states: 576 cases plus three warmups.
Each child is stopped before its first process notification event registration.
Event registration does not retroactively signal queued data. A repeat with a
new mismatching change stamp can queue data, but a nonempty existing queue does
not resignal merely because another data bit arrives. 579 children exited;
parent handle sets/counts remain stable after warmup. Both owned names and
descriptors were cleaned up. The existing selected state is unchanged.

The candidate event formula in check-fresh-event-model.py matches all 576
retained cases. It is not yet a general System namespace implementation.
Read-mask transitions (288 cases) and three-subscription sequences (288 cases) also match the retained-queue model, with manual resets between calls. Matching the current stamp does not clear an already queued data record. The additional 578 children exited and the original selected state is unchanged. Independent source and executable hashes were verified. No production code has been changed for this group.

| ID | Assumption | Basis | Dependent result | Stress / falsification probe | Status |
| --- | --- | --- | --- | --- | --- |
| S1 | The captured System descriptor can be evaluated with the measured generic-right mapping and untrusted mandatory label. | Actual 412-byte provisioning record and 3072 subscription calls. | System read admission. | Deny-only groups, unknown ACE/control/label, matching generic deny, restricted/AppContainer subject. | Retained pending adversarial parser tests. |
| S2 | Selected data arrival depends on change-stamp mismatch and queue state. | 576 fresh child cases across absent/present data and event ordering. | Guest event signal and pending mask. | Read-mask transitions, manual reset, duplicate mask and changed stamp. | Retained pending transition probe and implementation. |
| S3 | General System subscriber-level notifications can be inferred from private guest subscribers. | No supporting evidence; host System names may have external subscribers. | Write/subscriber masks on shared names. | Native global subscriber-information acquisition and separate namespace probes. | Rejected as a general assumption; remain explicitly unsupported until measured. |

High: ordinary process completion and notification delivery remain incomplete.
Medium: System write/subscriber-level events require separate global-state
semantics. These are blockers for general support, not evidence of success.

Implementation constraints identified before editing:

- Keep the Process-only descriptor parser and its native matrices intact.
  System descriptors require a separate narrow evaluator: relative descriptor,
  classic ordered allow/deny ACEs, measured generic read/write/all mapping, and
  either no SACL or exactly the measured untrusted mandatory label. Unknown
  controls, ACEs, labels, restricted or AppContainer subjects remain unavailable.
- System data subscriptions require the query snapshot. Existing catalog-only
  test fixtures deliberately lack that selection and must remain unavailable;
  runtime snapshot selection already provides it. Do not use absent data as an
  invented successful acquisition.
- System masks requiring write/subscriber-level state remain outside this
  initial read-subscription profile when rights would otherwise permit them.
  Native access denial must still precede that unsupported boundary. General
  host subscriber levels have not been captured.
- Read masks 0, 1, 16, 17 can use guest-owned generations and retained pending
  data. Keep output/probe semantics from the existing four-ABI service adapter.
  Retain queued data when a later request supplies the current stamp; subsequent
  mismatches do not resignal an existing queue. Dropping the data interest clears
  that retained bit. Registration does not signal an already queued bit.
- The read-mask transition and three-call probes each have 288 measured cases
  plus a warmup. Both preserve the existing System data and close 289 children.
  The first three-call producer failed compilation due to two missing printf
  arguments; version 2 corrects only that capture formatting and passes. Keep
  both source/build records, and use only version 2 as behavioral evidence.
