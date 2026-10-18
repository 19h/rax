# Captured TokenBnoIsolation

The query, captured metadata and output-store contract are implemented and validated below. Complete Windows process execution remains incomplete.

The four-profile baseline executes16680 native calls. Twelve private child controls add50040, for66720 calls. All use the same Windows10.0.29683 ARM64 kernel; x64 and x86 run through its compatibility layers. The child controller alters only its own children.

Microsoft winnt.h defines a PWSTR followed by BOOLEAN. Measured header extents are16 bytes for native64 and8 for WoW64. Disabled state writes a NULL pointer and zero Boolean while preserving padding. Enabled state retains the queried prefix in owned UTF-16 storage and rebases the outgoing pointer into the guest buffer. Required size is2*pointer_bytes +2*(prefix_units+1), with the tail absent when disabled. The capture allocation is bounded at65536 bytes, so the largest modeled prefix is(65536-16)/2-1=32759 UTF-16 code units.

Native capture, caller probing, token access and missing impersonation metadata are separate contracts. No host pointer or token handle is retained or exposed to guest execution. No guest query changes host token state.

`check-native.py` verifies raw producers, identities, normalized caller bytes and all66720 rows. `cases.json` has nine distinct measured row groups referenced by16 native runs. Pointer cells use explicit relocation markers only after the raw pointer bytes are verified against the native output buffer.

The final port has exactly the same 5737 selected source/data inputs as the tested revision5 prototype; its parent is the published namespace change.

## Caller order observed in the native matrix

| Profile | Caller header | Output validation | ReturnLength |
| --- | --- | --- | --- |
| ARM64, x64 | 16 bytes | Four-byte alignment and full supplied span, before token checks | Mandatory; probed before token checks |
| x86, x86 LARGEADDRESSAWARE | 8 bytes | Direct converted-field stores after token checks; unaligned stores accepted | Mandatory direct store after token checks, before output stores |

The WoW64 converter accepts a capacity of0xffffffff when its small converted
result fits. A short capacity does not touch its output buffer. Write-protected
guarded pages fail without consuming their guard; writable guarded stores
consume the reached guard. Native64 retains the existing probe ordering.

The first macOS prototype run passed five tests and failed the full replay at
the x86 maximum-capacity case. It reused the scalar-query full-span probe.
The second source epoch confines the measured direct-store behavior to
class44 WoW64 and reuses the existing direct-store guard helper. The complete
raw matrix remains asserted. Native64 live odd-alignment comparisons and
WoW64 aligned live comparisons are kept distinct; the raw WoW64 matrix
covers its differing alignment and fault behavior.


## Extended prefix-copy observations

Forty-six additional private child runs provide 95,088 class-44 calls. Each
run crosses the output page boundary at every byte offset from 0 through
`required_bytes + 32`, with four next-page protection states and three capacities.
The three capacities are the exact requirement, the requirement plus 16 bytes,
and `0xffffffff` bytes. The native producer records all bytes before the page
boundary, `ReturnLength`, returned status and remaining guard state.

WoW64 publishes the NULL pointer, then the enabled byte, then the rebased
pointer, then the prefix. Faults can therefore leave a valid header and only
part of the prefix. The observed copy order uses separate stores of at most
16 bytes. Its short-copy head/tail ordering and long-copy source alignment
are reproduced by the bounded store planner. For a prefix of N bytes, its
runtime and temporary storage are O(N/16), bounded by 65,520 payload bytes.
A whole-buffer preflight would erase these observed partial writes.

An independent check reconstructs every replay row from raw observations,
checks all 46 child lifetimes and 15 post-run source/executable identities,
and binds the normalized semantic fingerprint. Six corruption controls are
rejected even after input digests are rebound. The unmodified native matrix
and the extended copy matrix both pass the eight-test macOS prototype focus.
Final Windows/Linux and retained-consumer evidence is recorded separately below.

The store-order inference agrees with an optimized ARM64 NTDLL code region in
the captured system image. Its identity as the WoW64 converter's actual callee
has not been established. The native observations, not the disassembler's
nearest export label, own the byte-side-effect contract here. Other Windows
kernel builds and physical x64/x86 kernels are not represented by these runs.

Enabled-token private-directory routing depends on the separately selected
namespace paths. This query group captures the prefix but does not establish
execution of a complete process inside that private namespace. The bounded-scope
record tracks that remaining part of the broader process-emulation objective.

## Final validation and production startup

| Host | Full library passed/failed/ignored | Focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7929/0/2 | 8 | 168 | 544; native-memory suite excluded |
| Linux x86-64 container | 7923/0/2 | 8 | 168 | 544; native-memory suite excluded |
| Windows ARM64 | 7367/5/2 | 9 | 168 | Four native-memory cases; Unix corpus excluded |

All-target checks pass on all three hosts. Windows retains exactly the same five
lowerer failures as the preceding namespace group; the two ignored microkernel
tests remain explicit. The Windows focus includes an installed NTDLL comparison.
The two independent native matrices contain 66,720 and 95,088 calls, totaling
161,808 recorded observations. They run through the portable replay tests on all
three hosts. All four ABI profiles share Windows10.0.29683 ARM64; these records
are not physical x64/x86-kernel coverage.

The final source is ported onto RAX `d0a9b63c1109a915d425d385873bfc15776d3fe3`. Retained Assist checks
pass seven CTests and executable protection scans on macOS, plus five selected
C++ consumers each on Linux and Windows. Two actual Cargo-selected archives per
host have complete structural member walks and empty core feature sets; 428
selected owning source/fixture inputs match. Frozen Assist Rust carriers use
ABI184, while the compared root `e7fa3477850fbea5ce3ee819574f0a6946df172e` uses
ABI241. Nine selected C++ files match. This does not
establish a whole-current-root GUI/plugin/package build.

Actual outcomes from the unchanged ordinary observer:

- `terminal turn=2150070 Complete(Internal("native NTDLL service NtCreateMutant (0xba) at 0x7ff82fad1be0 is not implemented"))`
- `terminal turn=2152419 Complete(Internal("native NTDLL service NtCreateMutant (0xba) at 0x7ff82fad1be0 is not implemented"))`

Both ordinary programs return success from class44 and advance100 observer turns to unsupported NtCreateMutant. Neither reaches a successful guest exit.

A separate observer linked to the retained consumer's actual core archive
reaches the same two terminal outcomes. Controller status0 is not a guest exit.
The absent third helper is excluded from the execution count. Before/after
source, executable, library and program identities are bound in validation/.
Native, copy and final-validation corruption controls reject altered evidence
with rebound digests. `.gitattributes` and six representative Git-blob checks
preserve the raw evidence with Windows newline conversion enabled.

The root publication parent was refreshed to `78cb4c510e9989a9462ec027850b698ef8f17429` /
RustABI259. Eight selected RAX adapter and process-consumer
files still match the retained-carrier snapshot. `src/crypto/static_text.cpp`
changed independently to use the current Rust bridge; its pinned source identities
and scoped review are in `validation/upstream-source-drift-review.json`. That
new static-text implementation was not substituted into the frozen ABI184
carriers. Their test results remain specific to the compared snapshot. No
whole-current-root GUI/plugin/package result is inferred. The private source
diff is kept in the task cache, not copied into this public dependency record.
