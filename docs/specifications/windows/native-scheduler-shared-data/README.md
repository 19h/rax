# Native scheduler shared-data constructor and handle lifetime

`NtSetInformationProcess` class 112 (`ProcessSchedulerSharedData`) now creates a
distinct guest-owned `SchedulerSharedData` resource for the current process.
This is the constructor contract. Scheduler slot/data queries, mappings and
other process owners require their own measured implementation. No guest call,
pointer, handle or process-information buffer reaches the host NT setter.
Full same-host ordinary process execution remains an active incomplete goal.

The PHNT `ntpsapi.h` snapshot identifies class 112 but labels it query-only.
The actual build 29683.1000 setter returns success and a handle with object type
`SchedulerSharedData`, grant `0x1` and handle flags `0`. This measured setter
behavior owns the runtime contract; the conflicting declaration comment is
retained as evidence. Declaration arithmetic alone does not establish setter
behavior. Primary source, retrieval time, checksum and PHNT MIT license are
retained in `references/`; the checker verifies the enum position and comment.

| Constructor field | Native64 | WoW64 |
|---|---|---|
| InformationClass | 112 (`0x70`) | 112 (`0x70`) |
| ProcessInformationLength | exactly 8 B | exactly 4 B |
| Caller buffer | output-only pointer-sized handle | output-only 4 B handle |
| Nonzero-length output alignment | 4 B | byte alignment |
| Process rights | `PROCESS_SET_INFORMATION`, `0x200` | `0x200` |
| Created handle grant | `0x1` | `0x1` |
| Created handle flags | 0 | 0 |

`PROCESS_SET_LIMITED_INFORMATION` (`0x2000`) alone is insufficient. Pseudo-current
process, a current-process handle with `0x200` and the captured full-access
handle succeed. NULL/bad process handles return `STATUS_INVALID_HANDLE`;
pseudo-current thread and Event handles return `STATUS_OBJECT_TYPE_MISMATCH`.
Rights/type checks precede touching the output. Other guest process owners
remain explicitly unsupported after rights validation.

The original caller bytes are ignored. Zero and all-one 8 B initial values
produce identical status, ownership and guard effects, except the untouched
bytes outside a successful pointer-sized output store.

Native64 algorithm:

```text
nonzero length: enforce 4 B output alignment
nonzero length: reject overflow/out-of-user-range output extent
require exact length 8 B
validate process handle, type, rights and supported owner
probe the complete 8 B output for writing
create SchedulerSharedData(owner PID), open grant1 / flags0 handle
publish its 8 B identity -> STATUS_SUCCESS
```

WoW64 algorithm:

```text
require exact length 4 B
validate process handle, type, rights and supported owner
create SchedulerSharedData(owner PID), open grant1 / flags0 handle
copy the 4 B identity to the caller
copy succeeds -> STATUS_SUCCESS
copy faults -> fault status, unreturned live handle retained by process
```

Native64 probes before resource creation. WoW64's native kernel birth precedes
the final caller copy: an inaccessible output or a reached writable guard can
leave an unreturned handle. A writable reached guard is consumed once and
returns `STATUS_GUARD_PAGE_VIOLATION` (`0x80000001`). A read-only guarded
destination returns `STATUS_ACCESS_VIOLATION` (`0xC0000005`) without consuming
the guard. The local constructor wrapper preserves this captured distinction;
other service families retain their existing guard contracts. Short/wrong
lengths and denied handles preserve caller bytes and page protections.

```text
Native64: validate -> probe -> birth -> caller handle -> final close/drain
WoW64:    validate -> birth -> copy --+-> caller handle -> final close/drain
                                    +-> fault: hidden handle -> process drain
```

The guest object table owns the resource identity and PID. Existing generic
handle aliases share identity and grant; final close or process drain releases
the object. Tests assert lowest-free handle reuse, alias survival, final release
and hidden WoW64 birth cleanup. Native kernel pointer/reference counters are
implementation details and are not fabricated as guest kernel state. Specific
scheduler duplicate access/ACL policy beyond the existing generic contract is
unknown and remains outside this constructor increment.

Let H be open handles and T the existing memory-translation cost for one reached
page. At most two pages are visited by the 4/8 B output. Numeric range validation
is O(1); process-handle lookup is O(log(H+1)); lowest-free handle selection is
O(H). Constructor time is O(H + T), expected constant object-ID lookup, with
O(1) scratch and one fixed-size object/handle retained per successful birth.
Existing backing materialization can cover at most 2 * 4096 B = 8192 B. There
is no allocation proportional to the caller's ULONG length.

Five actual compiled producers bind mature and fresh observations. Four
caller profiles (ARM64, x64 compatibility, x86 WoW64 and x86 LAA) each execute
13 process inputs * 11 lengths * 14 output shapes * 2 initial contents =
4004 rows. Total: 4 * 4004 = 16016 independently reconstructed rows.
Native64 profiles create 18 handles each; WoW64 profiles create 84 each,
including 42 hidden copy-fault births. Total: 2 * 18 + 2 * 84 = 204 births.
Every mature row closes every newly created handle and restores the exact
native handle set; each producer also closes nine owned input handles.

The fresh ARM64 producer captures two complete cases at original installed
NTDLL service `0x1C` and true LR return, preceded by a class-53 startup call.
One explicit warmup is excluded. Entry bytes are `810300D4C0035FD6` (SVC 28;
RET) in the installed `MEM_IMAGE` allocation. Parent duplication verifies the
actual child handle type and grant, closes its duplicate and restores the
exact parent handle set. Native child exit 77 terminates the capture; it is not
an emulated guest exit. Fresh compatibility startup and physical x64-kernel
evidence remain unknown.

The current native kernel is 10.0.29683.1000 ARM64, principal SYSTEM
(`S-1-5-18`). Actual compiled source, PE machine/LAA, executable size/hash,
stdout and producer outcomes are bound by
`native-scheduler-shared-producer-identities.json`. Native NTDLL is 4609312 B,
machine `0xAA64`, SHA-256
`502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f`;
WoW64 NTDLL is 1849976 B, machine `0x014C`, SHA-256
`ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e`.
Other builds or DLL identities are not claimed equivalent.

The independent checker derives expected status, exact output bytes, protection,
resource count/grant/type/flags and lifetime from the declared input partition.
It verifies original fresh entry/LR/thread/arguments and exact source bindings.
Only the successful published native handle identity is related to the guest
handle identity in Rust replay; raw capture bytes are retained and validated.
No error byte, protection, hidden birth or unrelated field is masked.
Twenty-one controlled falsifications are rejected for their exact intended
reasons even after stdout rebinding. `evidence-manifest.json` binds the complete
corpus file set. Text captures are retained for reproducible independent replay;
no DLL, executable, kernel or archive binary is copied into this corpus.

`diagnostics/` preserves the aborted first fresh producer build and initial Rust
test/compiler/replay failures. These are excluded from positive native evidence
and from baseline runtime proof. `baseline-behavior.json` points to the preceding
published owning archive and unchanged observer at scheduler call 114140,
where three ordinary programs stop at unimplemented class 112 before guest exit.

Run the portable independent evidence gates with:

```text
python3 -B docs/specifications/windows/native-scheduler-shared-data/check-evidence.py
python3 -B docs/specifications/windows/native-scheduler-shared-data/check-negative-evidence.py
cargo test --locked --offline --no-default-features --lib native_scheduler_shared -- --nocapture
```

| Gate | macOS | Linux | Windows ARM64 |
|---|---|---|---|
| Constructor focus: pass/fail/ignored | 2/0/0 | 2/0/0 | 3/0/0 |
| Full library: pass/fail/ignored | 7737/0/2 | 7731/0/2 | 7148/5/2 |
| C API | 168/0/0 | 168/0/0 | 168/0/0 |
| All targets | build exit 0 | build exit 0 | build exit 0 |
| Windows integration target | 544/0/0 | 544/0/0 | 0 cases, cfg exclusion |
| Native memory integration target | 0 cases, cfg exclusion | 0 cases, cfg exclusion | 4/0/0 |
| Owning shipping/consumer gates | CLI/MCP, 7 CTests, 2 text scans pass | shipping + 5 C++ consumers pass | shipping + 5 C++ consumers pass |

Windows full-library exit 101 retains exactly the preceding four BZHI and one
FP16 lowering failures, listed in `validation-summary.json`. They are not
weakened, skipped or described as passing. Two ignored library tests retain
their existing status. Full Linux library and integration tests run serially;
the full macOS/Windows libraries use their recorded ordinary harness settings.
`cargo fmt --all --check` passes without mutating source files.

Owning validation uses frozen Assist roots
`e2fc5b483be7cad2dae17d889690cc398973c5e8` (Unix) and
`f386ed2aeac29a701d32f28eb73c14003eb55f7a` (Windows), current RAX sources and
nine current C++ consumers whose hashes match those embeddings. The complete
root delta and foreign dirty paths are recorded in
`scheduler-shared-owning-root-scope.json`. Whole current-root plugin, GUI and
package validation is excluded; scoped builds do not imply those gates ran.
No Assist tool schema, capability, persistent format or package default changes.
The public Rust `Object` enum gains `SchedulerSharedData`; its exhaustive wait
classification is updated. Opaque C API 1.11.0 and Assist ABI layouts are unchanged.

Actual native archives, selected by Cargo artifact metadata with empty core
features, pass complete structural member walks and 95 current Rust source
hashes plus two actual owning helper hashes:

| Archive | Size (B) | Members | SHA-256 |
|---|---:|---:|---|
| `assist_rs.lib` | 1126976278 | 4673 | `65804ba6aeef6f1816e691416870f0d5fe9e7100ca8e2e9dbf1a2781cf6014ca` |
| Selected RAX core `.rlib` | 739318028 | 260 | `2cbbd544ef73bd9bcb74299694f4885d28cf7e02097542db4c7a2fc8f73a4b28` |

The actual observer source is unchanged: 11177 B, SHA-256
`d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e`.
Production startup, instruction decoding, exception handling and the DLL entry
remain unmodified. Its scheduler terminal advances 114140 -> 115067:
115067 - 114140 = 927 scheduler calls. These are calls, not instructions or exits.

| Ordinary program | Current service | Class | Instruction PC | Stopped-thread PC | Result |
|---|---|---:|---|---|---|
| ARM64 smoke.exe | NtSetInformationThread, `0xD` | 57 (`0x39`) | `0x1800010D0` | `0x1800010D4` | reason 5, internal failure |
| ARM64 MSVCRT streams.exe | NtSetInformationThread, `0xD` | 57 (`0x39`) | `0x1800010D0` | `0x1800010D4` | reason 5, internal failure |
| ARM64 UCRT streams.exe | NtSetInformationThread, `0xD` | 57 (`0x39`) | `0x1800010D0` | `0x1800010D4` | reason 5, internal failure |
| Installed ARM64 whoami.exe | NtTraceEvent, `0x5E` | n/a | `0x180001620` | `0x180001624` | reason 5, internal failure |

All four report exit_code 0 in their internal-failure record; none proves guest
exit. The reached thread class takes 24 B and is a distinct subsequent service.
Constructor success does not substitute for that service or infer availability
from the PHNT name. The next increment must measure its own side effects.

```text
114140: NtSetInformationProcess(class112) -> typed handle birth, success
   |
   +-- unchanged ordinary NTDLL startup, +927 scheduler calls
   |
115067: NtSetInformationThread(class57) -> explicit unsupported frontier
```

Material assumptions A1/A2 are confirmed for captured constructor semantics and
guest-owned resource lifetime; A3 retains the native build/DLL boundary.
Each dependent result, stress test and falsification probe is in
`assumption-register.json`. `change-surface-map.json` covers every repository
execution plane, including Windows/macOS/Linux products and ABI consumers.
`bounded-scope.json` identifies reached next services as high-impact blockers
for the full goal, baseline Windows failures and uncaptured native/kernel
behavior as medium-impact limits, and the existing runtime record size as a
low-impact maintenance item. None is silently converted into success.

Final self-review verified append-only native export registration, exhaustive
object classification, probe/creation/publication order, hidden-birth drain,
exact native source/PE/entry/LR provenance, strategic test registration,
unchanged C ABI/features and exact owned worktree paths. User QG1-QG7 and RAX
QG1-QG9 pass for this constructor increment: no normative content, reconciled
assumptions, bounded scope, complete requirement/plane coverage, reproducible
units/ABI arithmetic, resolved declaration conflict and verified evidence.
These gates do not declare the full process-emulation goal complete.
