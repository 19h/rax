# Selected installed image-section mapping evidence

Native captures were collected on 2026-10-13. Final implementation and ordinary continuation validation completed on
2026-10-14. This increment does not establish full Windows process support.
The [runtime contract](../../../architecture/user-mode/selected-image-section-mapping.md)
owns the mapping, lifetime and projection description.

Current combined-source results are in [the upstream integration audit](upstream-integration/README.md).
The tables below retain the original mapping validation before that upstream change.

## Independent native observations

All four producer profiles ran under SYSTEM (S-1-5-18) on the same native ARM64
Windows kernel 10.0.29683.1000. x64 and x86 are compatibility processes; physical
AMD64/x86 kernels, other builds and other subjects are unknown. MSVC 18.7.1
compiled C++20 controls with static CRT. Actual executable PE Machine/LAA bits,
byte count and SHA-256 are captured independently of the build command.

| Family | Cases/profile | Profiles | Matrix observations |
|---|---:|---:|---:|
| Access, pointer faults and coupled rejection | 914 | 4 | 3656 |
| Geometry | 15 | 4 | 60 |
| Additional partial/large views | 35 | 4 | 140 |
| Image-information/header control | 4 | 4 | 16 |
| ImageMachine projection | 21 | 4 | 84 |
| Complete projected-byte comparison | 21 | 4 | 84 |
| Low-address placement | 145 | 4 | 580 |
| Retained-candidate collision matrix | 3 | 4 | 12 |
| Total matrix observations | | | 4632 |

The collision family additionally records three simultaneously held views per
profile. Each of the eight families on four profiles records a separate
close-surviving-view lifetime check: 8 x 4 = 32. These are not added to the
matrix total. The fresh original-entry collector measures two startup cases;
the startup-control collector measures seven. Each has one separately labelled
warmup. All nine measured startup children exit 77 with the exact original
parent handle set restored. The original ARM64 SVC40 call transports ten
arguments, returns through its true saved LR and maps selected kernel32 with
allocation 0x00800000 and protection 0x80.

The native ARM64 MapEx ImageMachine 0x8664 view equals the actual x64 raw mapped
view over every byte of both selected images, including zero-filled reserved
regions. The independently captured library file identities remain unchanged.

| Selected image | Compared bytes | Complete SHA-256 |
|---|---:|---|
| kernel32 | 1511424 | 5d964a7c733d54b6ec20e00b6968f7fe76dbc652bfa091abb70da0f367138939 |
| NTDLL | 4665344 | d964c7360c10a450719279e1609f85e3d5a098d66c77d9f3107fd18111c865d7 |
| Total | 6176768 | Two independent comparisons |

Whole-image equivalence is mapping evidence. Mixed ARM64EC/x64 instruction
execution and loader transitions require separate proof.

## Artifact binding and reproduction

Each family has source, a profiles PowerShell driver, build logs, native stdout,
identities, outcomes, a source freeze and an independent inspector. The fresh
and startup-control families use their specific ARM64 drivers. Drivers specify
the task VM paths in the captured environment; substitute paths deliberately,
format before capture, regenerate freezes and identities, and compile/run all
four profiles when collecting a new corpus. Never relabel old captures after a
source change. Native executables, DLLs and mapped bytes are not redistributed.
Failed earlier collectors remain outside this completed corpus in the task
cache; they cannot satisfy a final gate. The inherited executed owning PowerShell
driver retains its original trailing whitespace; altering its bytes would
invalidate the recorded executed-source identity.

From the repository root:

```sh
python3 -B docs/specifications/windows/native-section-map/verify-evidence.py
python3 -B docs/specifications/windows/native-section-map/check-final-validation.py
python3 -B docs/specifications/windows/native-section-map/stress-evidence-verifier.py
cargo test --locked --offline --no-default-features --lib native_section_map -- --nocapture
```

The verifier checks source/log bytes against native identities and source
freezes; actual executable identity against executed outcomes; actual PE
Machine/LAA, kernel, subject and selected-library identity; ordered complete
rows, producer exits, footer and lifetime counts; primary-source hashes; the
SDK status declaration; original-entry handle restoration; and complete
projection comparisons. It rejects optimized Python. The stress driver copies
the corpus into a temporary directory and applies nine controlled corruptions:
source byte, executed binary, kernel, subject, library, missing row with a
rehashed log identity, failed producer exit, LAA and optimized execution.
[evidence-falsification-results.json](evidence-falsification-results.json)
records all nine rejections. Rehashed missing-row rejection establishes that a
hash match alone does not establish matrix completeness.

Primary documents and redistribution notices are retained in references/;
[sources.json](references/sources.json) records URLs, retrieval UTC, bytes and
SHA-256. [native-sdk-map-status.json](native-sdk-map-status.json) records the
installed Windows SDK header identity and four exact status definitions.
Portable consumers use the shared checked service and VM implementation; no
guest handle, process, pointer, offset or mapping flag enters the host kernel.

## Validation scope

The portable replay executes 3656 access plus 580 low-address native-capture
rows: 4236 calls. PE bytes are synthetic with independently captured section
geometry/machine. ASLR-dependent scratch/free/upper-bound addresses are
explicitly normalized. Independently captured low-address occupied intervals
and a prior successful ZeroBits=1 call seed the low-placement candidate;
a later tighter-ceiling result is not used to choose that seed. The x86 LAA
fixture sets the real executable COFF flag before process creation.

The installed Windows leaf test executes actual ARM64/WoW64 NtMap/NtClose/
NtUnmap leaves, including x86 RET40, mapped headers and view lifetime. This
has a different scope from the synthetic replay and the native producer.

| Final source gates | macOS | Linux | Windows ARM64 |
|---|---|---|---|
| Focused mapping tests | 4 passed | 4 passed | 5 passed |
| Full library | 7763 passed, 2 ignored | 7757 passed, 2 ignored | 7178 passed, 5 baseline failures, 2 ignored |
| C API | 168 passed | 168 passed | 168 passed |
| All targets | Passed | Passed | Passed |
| Windows portable integration | 544 passed | 544 passed | Host-selected 0 tests |
| Windows memory integration | Host-selected 0 tests | Host-selected 0 tests | 4 passed |
| Owning archive and C++ consumers | Passed | Passed | Passed |
| Ordinary installed-library continuation | Previous host evidence separate | Previous host evidence separate | New NtQueryPerformanceCounter boundary; 0 guest exits |

Windows retains the same five pre-existing AArch64 SMIR lowering failures as
the section-open baseline; they are not attributed to mapping. The ignored
and host-elided rows are not runtime proof. Earlier macOS C API runs failed
at the unchanged 30 s native true budget; a separate retained resumable
control exited after 42.247493 s. Its dylib source freeze is unknown. The final
full C API run passed. Linux's first integration run had 543 passes and one
30 s subprocess timeout; its isolated repeat passed in 0.30 s. The complete repeat passed all 544 tests in 163.00 s. Original failures are retained.

Owning builds use frozen Assist roots e2fc5b48 (Unix) and f386ed2 (Windows),
124 current RAX Rust source units and nine byte-matching current C++ consumers.
This establishes the affected process/static-archive surface; it is not a
whole-current-root plugin, GUI, package or deployment claim. Core default
features remain disabled. [runtime-source-freeze.json](runtime-source-freeze.json)
binds the 20 changed Rust files. Final archive identities, same-source helper records and the unchanged
[ordinary observer](native-section-map-after-trace.rs) are retained.
[ordinary-continuation.json](ordinary-continuation.json) records
161784 -> 168227 turns, delta 6443, and four Internal outcomes. Three CRT
programs now stop at NtQueryPerformanceCounter (0x31), RVA0x1350 in selected
NTDLL; whoami retains NtTraceEvent (0x5E), RVA0x1620. Zero guest exits are proved.
The final Windows leaf also exercises the distinct cached AMD64 projection;
expanded collision assertions pass on all three hosts. A test-only final
projection assertion followed the full library runs; it changes no shipping
semantics and its five focused Windows tests were rerun. All owning sources
were reverified afterward.

## Assumptions and bounded findings

The [Assumption Register](assumption-register.json) records IDs, basis,
dependencies, stress tests, falsification probes and confirmed/retained/revised
statuses. [The surface map](change-surface-map.json) covers all 16 execution
planes. [Bounded findings](bounded-findings.json) separate host timing evidence,
projection/execution limits and remaining full-process requirements. No
nonblocking adjacent system was changed. Full ordinary Windows guest exits
remain unproved, so the overall three-platform process goal remains active.

Quality gates are evaluated against the measured increment. QG1 requires no
normative content; QG2 is the register; QG3 distinguishes completed and pending
requirements; QG4 preserves byte extents, pointer widths, timing and counts;
QG5 requires explicit faults/geometry/lifetime and unchanged baseline failures;
QG6 binds direct source/native/primary provenance; QG7 records bounded findings.
All required mapping-increment gates have direct evidence in
[validation-summary.json](validation-summary.json). Overall process completion
remains unproved; this is an increment of the active goal.
