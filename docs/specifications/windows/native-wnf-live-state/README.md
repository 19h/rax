# Native WNF live-state validation

The native typed-state snapshot test compared a current system-wide WNF record
with its historical stamp/value 1. The unchanged parent test fails on the current
Windows installation because native capture returns stamp/value 4. The retained
parent executable reproduces that failure independently, with matching hashes
before and after execution. The actor changing the system state is unknown.

This changes tests and CI selection. Native acquisition, guest records,
permissions, ABI, persistence and package behavior have no implementation change.
The system state is `0x0280032EA3BC0875`, with an eight-byte payload and type GUID
bytes `63 64 F9 48 D6 52 85 4E 9C 02 F2 C1 4E 17 26 4E`.

The repaired test calls the installed `NtQueryWnfStateData` export independently
before and after the complete snapshot. Both native observations must have the
same stamp and payload. In that interval, the retained snapshot must match the
complete native record exactly. Metadata maximum/type, empty private-process
state with stamp 0, and primary-subject assertions remain. The helper uses fixed
host-owned names, an aligned GUID and local output storage; it does not call the
production acquisition helper or mutate WNF state.

```text
independent native read -> complete snapshot -> independent native read
           |                                          |
           +---- same stamp and bytes required -------+
                           |
                 exact retained-record assertion
```

If either stamp or bytes change, the test retries the interval, at most eight
times. Exhaustion fails; it does not skip or fabricate a successful value.
For A attempts, N provisioned definitions and B retained bytes, the existing
capture contributes O(A * (N log N + B)) work. Here A <= 8; existing capture
limits remain unchanged. Each direct oracle uses a fixed eight-byte buffer.
The portable capture regression also checks value 4 and arbitrary payload bytes
with change stamp `0xFFFFFFFF`, alongside the historical value 1.

`tools/ci/process_embedding.py` now runs the WNF module first on each existing
Linux, macOS and Windows CI host. The prior process filter selected the sibling
process module and excluded WNF. Placing WNF first allows it to run before
unrelated process/profile failures stop the runner. Existing first/middle/last
failure-propagation tests still pass. No shell-specific command was added.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| WNF-A1 | Live system state can differ from a historical observation | Parent native failure observes stamp 4/data 4 instead of stamp1/data1 | Oracle-derived expectation | Nonhistorical bytes and maximum stamp | An immutable state contract rather than observed live values would invalidate this premise | Confirmed for observed failure |
| WNF-A2 | Matching independent stamp/byte observations provide a usable snapshot interval | Existing double-observation capture contract | Exact retained-record assertion | Changed stamp or payload, bounded interval exhaustion | Native brackets match but retained data differs, or eight intervals all change | Confirmed for the recorded native intervals |

| Impact | Remaining boundary | Task effect |
| --- | --- | --- |
| High | `NtReadVirtualMemory`, foreign-process queries and broader userland behavior | Separate from this test correction |
| High | Linux CI's exited guest TID procfs lookup | Cause remains separately recorded; no Linux model edit here |
| Medium | Five established Windows lowerer failures and x64 profile/time failures | This correction does not change those implementations or assertions |
| Medium | A continuously changing native state can exhaust eight intervals | Explicit failure rather than a false positive or skip |

| Gate | macOS ARM64 | Linux x64 container | Windows ARM64 |
| --- | --- | --- | --- |
| Complete WNF module | 12 passed | 12 passed | 14 passed |
| All targets | Passed | Passed | Passed |
| CI runner tests | Two passed | Two passed | Two passed with complete installed Python |

No selected test is ignored. Rustfmt 1.95 and the all-target Clippy check with
`x86_64-suite` pass. Windows's repaired test matches independent native stamp 4
and data 4 on attempt 0, and passes twelve separate process runs. Candidate and
retained-parent images keep their before/after hashes; the parent still fails.
Linux x64 execution uses translation on an ARM64 host.

The initial local runner check failed because the stripped embedded Python 3.14
lacks `_overlapped`, imported by `unittest.mock` through `asyncio`. The already
installed complete ARM64 Python 3.14 passes the same two tests with isolated
startup. Neither interpreter nor system configuration changes. The failed
prerequisite receipt remains separate from the successful continuation.

The source verifier binds 29139 inherited compiler inputs and a two-file overlay
from published parent `d5d8ea486c1c4e1b8be8d80d0be7698fa7200b02`. Publication-only
documentation is outside that compiler frame. Raw receipts retain their exact
bytes. The new unfiltered library suite was not rerun for this test-only change;
the parent Windows full run remains six failures, with this WNF failure isolated
and corrected by direct native tests. No green full-suite claim is made.

```sh
python3 -B docs/specifications/windows/native-wnf-live-state/check_evidence.py --source .
cargo +1.95.0 test --locked --offline --no-default-features --lib user::windows::wnf:: -- --nocapture --test-threads=1
python3 -B -m unittest discover -s tools/ci
```

The retained parent executable is a task-owned temporary control, removed after
its final attribution check and publication. Existing standalone/owning caches
have concrete next uses: Linux procfs and Windows lowerer/profile repairs, then
the recorded `NtReadVirtualMemory` frontier. The full same-host Linux/macOS/Windows
userland goal remains active.

The evidence verifier and full inherited-source check pass on macOS, Linux and
Windows. Six negative controls reject, including reindexed parent-exit,
repeat-count, image-stability and native-result changes plus optimized Python;
a restored positive passes and its temporary fixture is removed.

Quality gates: assumptions and finite retries are explicit; typed/native and
private-process requirements remain exact; CI covers the test module on all
configured hosts; units, provenance, failed prerequisites and remaining full-suite
limits are reconciled. No normative judgment or production-policy change is
required. The broader userland goal remains incomplete.
