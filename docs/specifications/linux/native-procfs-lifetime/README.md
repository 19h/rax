# Linux procfs thread lifetime

A retired guest thread's numeric `/proc/<tid>` path could resolve to a live
host task with the same number. `lookup` correctly stopped synthesizing the
guest entry after exit, but `owned` recognized only live IDs. The path resolver
then opened the host file. Parallel test-runner threads made the existing
`ENOENT` regression intermittent.

The controlled parent regression holds an owned native Linux thread alive,
assigns its TID to a guest thread, exits the guest, and opens the numeric status
path. Parent `e621a832adb9e9dcbe4173f8e467571f2e6b34c1` returns descriptor 3
instead of `-ENOENT` (`-2`) for the first guest ABI. The unchanged native status
path remains present while the repaired guest lookup returns `ENOENT`.

Linux's [`proc_pid_lookup` and `proc_task_lookup`](https://raw.githubusercontent.com/torvalds/linux/v6.19/fs/proc/base.c)
resolve live tasks in the procfs mount's PID namespace and return `ENOENT` when
the required task is absent. The [kernel procfs documentation](https://docs.kernel.org/filesystems/proc.html)
also describes task lifetime and numeric PID reuse. This change concerns path
lookup; it does not infer an open descriptor's lifetime or error from a missing
pathname.

```text
guest clone -> record issued TID -> guest exit -> synthesized entry absent
                       |                              |
                       +---- retain exact ID ---------+
                                      |
                         owned path -> ENOENT

live guest / known process link -> existing lookup and routing
unissued numeric ID             -> existing routing
forked child                    -> reset history to its own leader
exec                            -> retain issued history
```

`ThreadIds` stores disjoint inclusive ranges of exactly issued identities.
Insertion merges adjacent ranges and never fills an unissued gap. For R ranges,
membership and insertion cost O(log R), and retained storage costs O(R).
Sequential allocation retains one range; 10,000 sequential identities are tested.
The retired-path guard also checks current threads and known process links, whose
existing lookup/routing takes precedence. Its additional work is O(log R + T + L)
for T current threads and L known links; the existing lookup remains first.

The history starts with the leader, records successful thread creation, survives
thread exit and exec, and resets after a host fork becomes the new guest child.
It does not alter TID allocation, credentials, host procfs, namespaces, permissions,
or unrelated foreign-process paths. The public C ABI and package layouts are
unchanged. This is internal Rust process state, without a persisted schema.

The portable tests exercise all five supported Linux guest ABIs through the
existing harness. They cover sparse `clone3.set_tid`, unissued gaps, numeric and
task aliases, live reuse, nonleader exec, integer boundaries, merging, duplicates,
and compact sequential histories. Unix tests additionally preserve the existing
route for a known process relationship reusing a retired number. The Linux-only
test holds a real native task alive and checks bare-directory/status/fd opens and
an executable-link lookup after guest exit. The original lifetime assertions
remain unchanged.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| LP-A1 | A retired guest TID must not resolve to an unrelated live host task | Resolver trace, parent failure and Linux procfs lookup contract | Retired-path ownership | An owned host thread deliberately collides with the guest TID | Guest opens the native path after guest exit | Confirmed by controlled parent failure and candidate check |
| LP-A2 | Exact issued history survives exec, resets in a forked child and yields to current guest identities or known links | Existing process lifecycle and sparse-ID allocation | History representation and lifecycle hooks | Sparse IDs, reuse, nonleader exec, reset and known-link priority | Unissued gap claimed, live entry hidden or old parent history retained after reset | Confirmed for exercised paths; full fork suites retain native coverage |

The initial assumption register mentioned range splitting. Review established
that splitting is unnecessary: current identities and known links keep their
existing routes without deleting historical membership. The retained refinement
records that change explicitly.

| Plane | Effect and evidence boundary |
| --- | --- |
| Plugin lifecycle, UI, conversation, agent backend, prompt/context | No changed contract; callers retain the process API |
| Tools, permissions, main-thread dispatch, Mesh/MCP, transport/crypto | No schema, dispatch, approval or transport changes |
| Persistence, SDK/ABI, update/release | No serialization, public header, version or package-layout changes |
| Optional engines | Embedded RAX's Linux guest procfs changes; other guest OS models are unchanged |
| Targets/platforms | Shared Linux model compiled on macOS, Linux and Windows; actual host collision exercised on Linux; retained Assist carriers and C++ consumers checked separately |
| Tests/docs | Strategic lifetime regressions and frozen raw evidence; no source-presence test |

| RAX plane | Effect and reason |
| --- | --- |
| Direct decode/execute, CPU register state | Unchanged; the new state belongs to the Linux process personality |
| Memory/MMU | Guest address-space, permission and fault mechanics are unchanged; pathname ownership changes before host filesystem resolution |
| SMIR lift/IR/interpreter, optimizer, native lowerers, JIT | Unchanged; no instruction representation, optimization or admission path consumes the new history |
| Backend, machine/device | Unchanged; no backend state conversion, board or device behavior changes |
| Oracle/analysis | Unchanged; no decode, effects, analysis or completeness output changes |
| C ABI | Header bytes unchanged; internal Rust process state has no C layout or serialization contract |
| Tests/docs | Library regressions, complete `user_linux` target, existing C API/integration gates and owning consumers |

Validation results are recorded in `proof.json` and checked by
`check_evidence.py`. Source verification binds 29,141 inputs and an eight-file
overlay to the published parent; publication-only documentation is outside that
frame. Six inherited tracked files change and two Rust files are new. Link text
is hashed as link text. Windows's previously recorded native Cargo configuration
projection is the only platform-specific input exception.

| Gate | macOS ARM64 | Linux x64 container | Windows ARM64 |
| --- | --- | --- | --- |
| Focused procfs tests | 8 passed | 9 passed | 6 passed |
| Unfiltered library | 8,184 passed, 0 failed, 2 ignored | 8,179 passed, 0 failed, 2 ignored | 7,655 passed, 5 failed, 2 ignored |
| Complete `user_linux` target | 42 passed, 1 ignored | 42 passed, 1 ignored | Existing Unix-only runtime target; compilation gate recorded separately |
| C API | 183 passed | 183 passed | 183 passed |
| Windows integration target | 544 passed | 544 passed | 4 passed |
| All targets | Passed | Passed | Passed |
| Actual retained Assist consumers | Eight targets, seven CTests passed | Five C++ consumers passed | Five C++ consumers passed |

All three owning carriers retain identical selected core/carrier archive hashes
before and after their C++ validation, including structural archive-member walks.
The initial owning manifest inherited old provenance labels; a supplemental
manifest corrects those labels while retaining identical 3,974 source rows and
helper hashes. Original receipts remain unchanged.

The five Windows failures are exactly the established AArch64 lowerer failures;
the repaired WNF test passes on this ARM64 installation. The `user_linux` live
Docker oracle is the one existing ignored test on each Unix host. Recorded
program fixtures match all 97 cases in each of five configured modes; the
`--no-default-features` selection does not establish optional native JIT
admission or an independent live Docker oracle.

Rustfmt 1.95 and strict library Clippy pass. The ordinary all-target Clippy check
with `x86_64-suite` passes. An additional strict all-target invocation fails on
944 diagnostics from unchanged x86 instruction tests; its raw failed receipt is
retained. No warning suppression or shared test assertion is changed.

The Linux runtime suite uses a task-owned native Linux filesystem mounted at an
in-worktree temporary path. macOS bind mounts do not supply all native Linux
filesystem semantics required by the existing suite. Linux x64 execution uses
translation on an ARM64 host; it does not establish physical x64 instruction
behavior. Windows native execution uses the existing ARM64 VM without desktop
interaction. Retained Assist ABI184 consumers are distinct from current ABI274
shipping products and their CI.

| Impact | Separate finding | Blocks this changeset? |
| --- | --- | --- |
| High | x64 CI's WNF snapshot lacks the expected private-process record; cause unknown | No; no WNF model or assertion changes here. Blocks a full Windows parity claim |
| High | `NtReadVirtualMemory` and broader same-host userland remain incomplete | No; full goal remains active |
| Medium | Existing TID allocation overflow/domain and cross-process collision policy require separate review | No; this change preserves allocation policy |
| Medium | Five established ARM64 Windows lowerer failures and x64 profile/time failures | No; full-suite failures remain explicit |

```sh
python3 -B docs/specifications/linux/native-procfs-lifetime/check_evidence.py --source .
cargo +1.95.0 test --locked --offline --no-default-features --lib procfs -- --nocapture --test-threads=1
```

No normative judgment is required. The evidence verifier checks exact selected
counts, failures, logs, source inputs and archive stability. The complete
same-host Linux/macOS/Windows userland goal remains incomplete.

Nine negative controls reject reindexed parent/result/failure/image/archive/
source-provenance corruptions and optimized Python. A restored positive passes;
the temporary control tree is removed. `negative-controls.json` binds these
results to the exact checker digest. The completed native Linux temporary volume
and macOS test scratch data are removed. Existing in-worktree build caches have
concrete next uses in the remaining Windows lowerer/profile, private WNF and
read-service work.
