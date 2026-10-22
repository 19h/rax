# Native WNF provisioning expectations

The native snapshot test required the process-private name
`0x41911A28A3BC10B5` unconditionally. The Windows ARM64 installation provisions
that name; the x64 CI snapshot has no selected record and fails at the fixed
lookup's `unwrap`. The production snapshot itself succeeds, including the
independently checked typed system state. Whether the x64 registry lacks the
name or acquisition dropped it remains unknown until the new independent
registry observation executes on that host.

This changes test expectations and adds an independent read-only native oracle.
Production acquisition, guest state, permissions, ABI and packages are unchanged.
`RegGetValueW` reads only the fixed HKLM Notifications value using the binary
type restriction and a 65,536-byte buffer. Successful reads retain their complete
raw metadata; `ERROR_FILE_NOT_FOUND` represents absence. Every other native
status fails the test. No host value, WNF state, token or configuration is changed.
The API contract comes from [Microsoft's RegGetValueW documentation](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-reggetvaluew),
whose exact retrieved bytes and provenance are retained here.

```text
private registry read -> typed state -> complete snapshot -> typed state
         |                                                  |
         +---------------- private registry read -----------+
                                  |
                   stable raw metadata and typed state
                                  |
         definition present ------+------ definition absent
                  |                            |
      exact empty guest state         exact absent guest record
           with stamp 0                 no fabricated state
```

The complete metadata and typed-state observations must match their respective
brackets. Changed intervals retry at most eight times; exhaustion fails. An
independently present definition still requires the exact previous guest-state
assertion: stamp 0 and empty data. An independently absent definition requires
the guest record to be absent. A lost provisioned record and a fabricated
unprovisioned record both fail. The typed system name `0x0280032EA3BC0875`, its
eight-byte maximum, exact GUID, native stamp/payload comparison and primary
subject assertion remain mandatory.

The existing portable capture regression now explicitly checks that selecting
the system definition does not fabricate the missing private definition. Its
private-payload isolation case remains: native admission with a host change
stamp or nonempty reported size starts the new guest's process-private state
empty at stamp 0.

For A attempts and bounded metadata size M, the added native reads cost O(A*M)
time and O(M) retained scratch, with A <= 8 and M <= 65,536 bytes. The existing
complete-snapshot complexity and budgets are unchanged. The raw metadata is
test-owned storage; guest requests do not enter this oracle.

An independent Python/Win32 probe on Windows 11 ARM64 build 29683 observes the
private definition as a 72-byte binary value, with identical before/after bytes.
Its independent `NtQueryWnfStateData` call uses a NULL output and zero capacity
and returns success, stamp 0 and zero bytes. The typed system read returns
stamp 4 and payload `04 00 00 00 00 00 00 00`. The fixed missing-name control is
absent. This is an ARM64 positive control; it does not establish the x64 host's
registry contents.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| WP-A1 | An installed provisioning catalog can omit the fixture's private name | x64 snapshot succeeds but selected lookup is absent; ARM64 native value is present | Independent presence expectation | Missing definition, dropped present record and fabricated absent record | Native registry reports present while selected lookup is absent; the new test still fails | Retained pending direct x64 observation |
| WP-A2 | Stable complete registry metadata brackets identify the tested fixture presence | Fixed read-only API and existing bounded snapshot contract | Native consistency interval | Changed bytes, missing/present transition, unexpected status, maximum buffer | Matching native brackets disagree with selected presence, or eight intervals change | Confirmed for recorded ARM64 interval; other profiles remain under validation |

| Plane | Effect and evidence boundary |
| --- | --- |
| Native WNF tests | Independent registry oracle, bounded consistency and exact presence/absence expectations |
| Shared guest semantics | Existing private-state initialization and missing-definition behavior remain unchanged; portable assertions retain them |
| Direct ISA, CPU state, memory/MMU, SMIR, optimizer, lowerers, JIT, backend and devices | No instruction, state conversion, memory, admission or device implementation changes |
| Oracle/analysis and C ABI | No public analysis output, C header or ABI layout changes |
| Assist lifecycle, UI, conversations, tools, permissions, dispatch, Mesh/MCP, transport, persistence and SDK | No changed production contract |
| Targets/platforms | Test module compiles on all three hosts; Windows executes the native API oracle, Unix hosts execute the shared capture checks |
| Build/CI/package | Existing WNF-first CI selector remains; no workflow, dependency, feature-default or package-layout change |

| Impact | Separate finding | Effect |
| --- | --- | --- |
| High | Independent x64 registry observation is not yet available in the retained baseline | No cause or x64 parity claim before that evidence |
| High | `NtReadVirtualMemory` and broader same-host userland remain incomplete | Full goal remains active |
| Medium | Five ARM64 lowerer failures and x64 profile/time assertions | Unchanged implementations and explicit separate failures |
| Medium | Continuous native provisioning changes can exhaust the eight intervals | Explicit failure; no skip or invented fixture |

The exact compiler frame is inherited from parent
`945ace5d5de706da96418c8b939ede07c0166ae1`, with 29,141 inputs and one Rust
test-file overlay. Publication-only documentation is outside that frame.
The parent procfs changeset's owning ABI184 consumers and current Assist ABI274
shipping CI remain separate evidence. No production-consumer rebuild claim
follows from a test-only edit.

| Gate | macOS ARM64 | Linux x64 container | Windows ARM64 |
| --- | --- | --- | --- |
| Complete WNF module | 12 passed | 12 passed | 14 passed |
| Unfiltered library | 8,184 passed, 0 failed, 2 ignored | 8,179 passed, 0 failed, 2 ignored | 7,655 passed, 5 failed, 2 ignored |
| C API | 183 passed | 183 passed | 183 passed |
| Windows integration targets | 544 passed | 544 passed | 4 passed |
| All targets | Passed | Passed | Passed |

The completed native Linux temporary volume and macOS test scratch data are
removed. Existing in-worktree build caches have concrete next uses in the
remaining Windows lowerer/profile and native read-service work.

Rustfmt 1.95 and the ordinary all-target Clippy check with `x86_64-suite` pass.
The Windows native oracle reports the independent 72-byte provisioning value
on attempt 0. No selected WNF test is ignored. Linux x64 execution uses
translation on an ARM64 host and does not establish physical x64 instruction
behavior. The Windows Server 2022 x64 baseline is an independent native CI host;
its new registry-presence oracle remains pending after publication.

Nine negative controls reject reindexed parent, registry-bracket, private-stamp,
exit/failure-list, image and source-parent corruptions plus optimized Python.
A restored positive passes; its temporary tree is removed. The result binds the
exact checker digest in `negative-controls.json`.

No normative judgment is required. Native provisioning, guest-private state,
observed controls, assumptions and remaining unknowns are distinguished. The
full Linux/macOS/Windows userland goal remains incomplete.
