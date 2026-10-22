# Current-process native virtual-memory reads

`NtReadVirtualMemory` was unsupported in the installed-NTDLL path. The retained
parent regression enters the actual kernel-service dispatcher and fails before
copying a guest byte. The service now reads the guest process's own mapped
address space, including installed image pages, and publishes its pointer-width
count. Both installed service resolution and built-in imports use one handler.
The appended export table preserves every previous ordinal and trap RVA.

The pinned PHNT signature and its license are identified in `sources.json`.
Behavior comes from original installed NTDLL on Windows 11 ARM64 build 29683,
with four execution views: ARM64, x64, x86 and x86 LAA. These are three guest ABIs
on one kernel. They do not establish independent x64-kernel behavior. Immutable
sources and launch receipts bind compiler selection, PE machine/LAA fields,
executable bytes and eight runtime DLL identities before and after execution.

```text
capture arguments -> extent checks -> optional count check
                                           |
                        zero length -------+------ nonzero length
                            |                         |
                        count = 0               process read grant
                                                      |
                                       actual guest source mappings
                                                      |
                                           bounded source snapshot
                                                      |
                                          destination store sequence
                                                      |
                       NT status <---- late pointer-width count store
```

| Surface | Current measured contract |
| --- | --- |
| Ownership | Current guest process only; real process handles require `PROCESS_VM_READ` (`0x10`) |
| Handles | Exact invalid-handle, object-type and access-denied results; zero-length reads do not reference the handle |
| Arithmetic | Checked half-open source/output extents; native upper-user failures precede count publication; WoW64 pointers are zero-extended |
| Source | Actual committed protection authorizes reads; execute-only source pages can be read; source guards stay armed |
| Reported protection | Query metadata cannot authorize access to an inaccessible actual mapping |
| Destination | Reached guard is consumed; faults return `STATUS_PARTIAL_COPY` (`0x8000000D`) |
| Native count | Optional unaligned `SIZE_T` slot checked before copying; faults return the observed access/guard status |
| WoW64 count | Pre-read optional `ULONG`; a failed read consumes its reached guard and suppresses publication; readable read-only/execute-only slots fail their late store after copying |
| Aliases | Capture all arguments before writes; publish count last, including native source/output overwrites |
| Partial count | Can exceed both stored bytes and writable prefix; reflects the reached faulting store address |
| Large copies | Fully accessible overlap exposes 65,536-byte snapshot boundaries; faulting spans publish completed 14-page blocks: `14 * 4096 = 57,344` bytes |
| Alignment | Below 512 bytes, measured loop origin is independent of caller source alignment; at and above 512 bytes, retain source byte alignment with a 16-byte leading store |
| RTL conversion | Shared `STATUS_PARTIAL_COPY` maps to `ERROR_PARTIAL_COPY` (299), preserving the existing guard-warning conversion |
| Foreign process | Explicit unsupported frontier before caller mutation; guest addresses never enter a host process-memory API |

An eight-byte store with four writable bytes returns partial-copy/count 4 and
leaves the destination unchanged. A 65-byte copy with 32 writable bytes stores
that prefix and next attempts offset 33, yielding count 33. A crossing three-byte
copy publishes no destination bytes. Byte loops and a presumed written-prefix
count therefore disagree with the native profile.

The alignment distinction is observable with a source shifted by one byte:
512 bytes requested with 31 writable output bytes natively writes all 31. The
previous compiled handler wrote only 24. The retained actual owning-core
regression fails before the correction and passes afterward. The v7 producer
exercises every source byte alignment with two patterns and lengths on both
sides of the threshold. Kernel allocation/locking internals remain unknown;
these are measured store effects, not a claim about undocumented kernel code.

The source walk validates actual guest commitment/protection before a narrow
address-space permission bypass for kernel-mediated reads. Destination stores
retain ordinary guest write checks. Metadata walks do not populate frames or
consume guards. Reached accesses use existing demand-page machinery. For N bytes,
P visited pages and R allocation/page-state runs, work is O(N + P log R), with
at most 65,536 bytes of scratch. Checked extents bound subsequent arithmetic.

| Producer | Calls per view | Purpose |
| --- | ---: | --- |
| v1 | 1,528 | Grants, handles, extreme/zero sizes, pointer faults, protection roles, count aliases and guards |
| v2 | 520 | Large source/output failures and overlapping copies |
| v3 | 99 | Large alias boundaries, crossing stores and lazy pages |
| v4 | 609 | 21 output prefixes by 29 lengths, aligned source |
| v6 | 609 | Same boundaries with source shifted by one byte |
| v7 | 1,920 | 16 source alignments, two patterns, 12 prefixes and five threshold lengths |
| Read total, four views | 21,140 | Complete status, byte and protection replay |
| v5 RTL | 7 | Independent conversions for statuses produced by the service |
| RTL total, four views | 28 | Exact error values and retained last error |

Every native call is repeated with the same hashed executable and stable DLLs.
All non-residency row fields match. Every shared replay reconstructs complete
three-arena preimages, applies all changed-byte runs or lossless repeated blocks,
and compares every resulting byte, status, observed protection and callback-frame
state. The x86 LAA fixture sets the PE flag and verifies its guest address ceiling.
Immutable preimages are cached and cloned; successful afterimages use complete
byte equality. No region or failed byte is omitted for test performance.

Raw working-set bits remain preserved. Native kernel prefetch sometimes makes
pages outside the request resident, so host prefetch breadth is diagnostic,
rather than a guest paging contract. No byte, status or protection is normalized.
Separate regressions verify reached guest-page residency, source guards,
reported/actual protection disagreement and the untouched foreign frontier.
The Windows installed-leaf test executes selected NTDLL instructions and checks
image bytes, count width, untouched tails and retained source guards.

The RTL regression independently observes the old fallback 317 in an actual
owning archive and fails against native 299; the corrected archive passes.
The original unsupported-service regression, intermediate failures and deliberately
superseded runs remain distinct from final acceptance evidence.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| RM-A1 | A self-read uses guest mappings and existing grants | Guest/object ownership plus original handle cases | Ownership boundary | Foreign read grant with armed caller guard; differing reported protection | Foreign completion, consumed caller guard at frontier or metadata-authorized inaccessible read | Confirmed for the tested boundary |
| RM-A2 | Store/count order follows the selected native profile | 21,140 calls, 28 conversions and complete repeats | Copy/fault/alias behavior | Bounds, guards, slot protections, aliases and partial stores | Any complete byte/status/protection mismatch | Confirmed for this profile; other kernels remain separate |
| RM-A3 | Measured alignment effects suffice without assuming kernel allocation internals | Independent v6/v7 cases and compiled before/after regression | Destination loop | All 16 alignments, two patterns, 511/512/513 bytes | Any native shifted-source afterimage/count mismatch | Confirmed for recorded effects; internal kernel mechanism unknown |

| Plane | Effect and evidence boundary |
| --- | --- |
| Windows dispatch and memory | New shared service/export family; narrow actual-page-state accessor; existing guard/residency ownership |
| Status translation | Additive shared NT status and measured DOS mapping |
| ISA, CPU state, SMIR, optimizer, lowerers, JIT and devices | No instruction/state/lowering/device implementation change |
| Linux/macOS process adapters | Unchanged; all three hosts compile and exercise the same Windows guest service |
| C API and analysis | Existing process API; C header, layout and analysis schema unchanged |
| Assist lifecycle, UI, conversation, prompt, tools, permissions, Mesh/MCP, transport, persistence and SDK | Existing consumer contract; no new setting, schema, permission or external effect |
| Targets/platforms | Library, C API, integration/all-target gates and actual owning consumers on all three hosts |
| Packaging | Linked service; capture fixtures are test inputs, with no new runtime asset |

| Impact | Remaining finding | Blocking boundary |
| --- | --- | --- |
| High | Foreign process address-space ownership is unmodeled | Explicit frontier; blocks full ownership claim, not self-read support |
| High | Broader same-host process userland is incomplete | Ordinary owning probes establish the next actual boundary; whole goal remains active |
| Medium | One ARM64 kernel profile underlies the four native views | Independent x64-kernel behavior remains unknown |
| Medium | Existing ARM64 lowerer and x64 profile/time failures | Retain exact failures; do not skip or weaken assertions |

The final compiler frame has 29,173 inputs: 12 Rust and 28 native-capture
overlays against parent `26ebabe4f8edefd87fc26630588cce0024b5bff8`.
Its SHA-256 is
`02b8f1cf7b693e6e379b2f8360e1dd9f646248f41dba8699d692209fc99f8303`.
The retained actual Assist ABI184 linked consumers and current Assist ABI274
shipping CI remain separate proof surfaces. Complete gate counts, actual
core/carrier structural walks, ordinary terminal results, cleanup and rejected
evidence corruptions are bound by `proof.json` and `check_evidence.py`.

The original Linux full test command passed, then its source-integrity postcheck
failed because the active worktree had disappeared. Cause is unknown. The exact
parent plus frozen 40-file overlay reconstructed all 29,173 compiler inputs.
The original failed receipt remains immutable. Final Linux acceptance requires
a complete post-restoration rerun with input checks before and after every gate;
it does not reuse the interrupted integrity result. macOS and actual owning
checks completed before removal, and the native Windows source was unaffected.

Final unfiltered library results are macOS 8,196 passed/0 failed/2 ignored,
Linux 8,191/0/2 and Windows 7,668/5/2. Windows failures are exactly the five
retained ARM64 lowerer failures. C API 183, integration 544 on each Unix host
and 4 on Windows, and all-target checks pass. The complete post-restoration
Linux run passes every gate and its source-integrity checks. Nine data
corruptions and both optimized-Python modes reject; the restored positive
passes and its temporary evidence copy is removed.

The final verifier passes on macOS, Linux and native Windows against one frozen
proof/checker pair. `validation/manifest.json` and its raw host receipts attest
that pair. This output directory is separate from the indexed verifier inputs
to avoid self-reference. Run `python3 -B validation/check_validation.py` after
`check_evidence.py` to verify those receipts and the thirteen control outcomes.
