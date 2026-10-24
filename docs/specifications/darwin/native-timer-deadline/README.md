# Native Mach timer deadline units

The `mk_timer` fixture tests that cancelling a rearmed critical timer returns
the latest deadline. A reported tick is not necessarily one kernel hardware
period. Rosetta reports a 1/1 ns timebase over the measured 24 MHz counter;
the native ARM64 timebase is 125/3 ns per tick.

The comparison now permits a downward conversion loss of at most one hardware
period, derived from `mach_timebase_info` and the exact eight-byte
`hw.tbfrequency` query. A missing, zero or incorrectly sized frequency result
fails the predicate. For the future deadline under test, zero output, a value
later than requested, the earlier rearm deadline and a loss beyond the derived
period are rejected.
All other fixture output and the timer runtime implementation are unchanged.
The comparison takes constant time and space; the native driver uses `O(N)`
time and space for `N = 512` observations per bounded cohort.

For a reported timebase `n/d ns/tick` and frequency `f hardware ticks/s`:

```text
q = ceil((10^9 ns/s / f hardware ticks/s) * (d reported ticks / n ns))
  = ceil(10^9 * d / (f * n)) reported ticks per hardware period

Rosetta: q = ceil(10^9 / 24,000,000) = 42 reported ticks = 42 ns
ARM64:   q = ceil(10^9 * 3 / (24,000,000 * 125)) = 1 reported tick

accepted: observed <= expected && expected - observed <= q
```

The C expression uses unsigned 128-bit intermediates; its maximum denominator
is below `2^96`, and the numerator is below `2^62`. The subtraction occurs only
after establishing order. This bounds rounding from unit conversion rather
than introducing a sleep or scheduler tolerance.

## Independent native observations

`native-timer-cancel.c` owns one Mach timer, rearms it from 10 s to 60 s into
the future, cancels immediately, and destroys it. Its 512 cases add successive
reported-tick phases to exercise representable deadlines between hardware
ticks. No future deadline is awaited. The producer SHA-256 is
`0e0647c7f350e4f1da46c4cb68dc1108f956442b69db071b18de38ae1014dfa5`.

Run the bounded control with:

```text
python3 docs/specifications/darwin/native-timer-deadline/run.py
```

The driver builds both actual Mach-O architectures, queries their respective
hardware frequency and runs each at default and utility QoS. Its temporary
directory and all compiler temporaries remain inside this worktree and are
removed after the subprocesses finish. Native call errors, incomplete captures,
wrong architectures, failed cleanup and losses outside the hardware-period
bound fail the control. It reports relative deltas, timebase metadata and
source/binary hashes. It also reports whether the observed values match the
double-floor conversion, independently of the fixture predicate.

The final local driver run on macOS 27.2 reports:

| Architecture | QoS | Cases | Delta in reported ticks | Outside old one-tick limit | Double-floor mismatches |
| --- | --- | --- | --- | --- | --- |
| ARM64 | default | 512 | 0 | 0 | 0 |
| ARM64 | utility | 512 | 0 | 0 | 0 |
| x86-64 through Rosetta | default | 512 | -42 through 0 | 503 | 0 |
| x86-64 through Rosetta | utility | 512 | -42 through 0 | 493 | 0 |

For the measured Rosetta profile the exact observed conversion is
`floor(floor(deadline_ns * 3 / 125) * 125 / 3)`. The native ARM64 observations
retain every requested tick. An earlier independent cohort has the same range
and zero conversion mismatches; 989 of its 1,024 Rosetta cases exceed one
reported tick. Case counts within the range vary with clock phase.

The actual compiled fixture helper passes 5,120 corpus/control comparisons
per architecture: all 1,024 earlier native observations, plus zero output,
one tick into the future, one tick beyond the permitted loss and the wrong
deadline 50 s earlier. All four native-versus-emulated `mk_timer` fixtures pass
with `RAX_USER_DARWIN_REQUIRE_ORACLE=1`: ARM64 and x86-64, default and utility
QoS. There are four passed, zero failed, zero ignored and 113 filtered tests.

Three local executions of the original complete x86-64 utility fixture also
pass. Its unphased deadlines happen to round by zero or one tick here. The
phase-controlled observations falsify its general one-reported-tick assumption;
they do not claim reproduction of the whole hosted failure. The preceding
[hosted Darwin job](https://github.com/HexRaysSA/rax/actions/runs/40082067045/job/120837493221)
has 116 passed and one failed, with native predicate zero versus emulated one.
That job did not record its raw delta. The native driver now runs before the
hosted fixture suite so that profile can be measured directly.

## Assumptions and scope

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| MT1 | One reported tick bounds a cancelled critical deadline's conversion loss. | Earlier fixture predicate. | Native reference comparison. | Both ABIs, all 512 phases, two QoS profiles. | A successful native call loses more than one reported tick. | Falsified for Rosetta |
| MT2 | One hardware period bounds the measured downward conversion loss. | Native timebase/frequency controls and all recorded phases. | Revised comparison. | Native and translated clocks; zero, future and excessive-loss adversarial outputs. | The native driver observes loss outside the derived period. | Confirmed for the recorded profiles; future builds remain measured |
| MT3 | The hosted predicate mismatch has this same conversion cause. | Hosted native zero/emulated one and independent native rounding evidence. | Hosted causal attribution. | Raw hosted delta and timebase before the fixture suite. | Hosted status/delta establishes another cause. | Retained inference; raw hosted delta unknown |

The changed C fixture and native producer require Mach APIs and a macOS kernel.
They are native-oracle inputs, not shipping code or Windows/Linux build inputs.
The existing oracle explicitly fails when required on an unavailable host;
the new driver also rejects non-macOS hosts. No platform skip is added. Shared
Rust timer semantics, target membership, optional features, public C API and
Assist ABI remain unchanged from their already compiled three-host frame.

High impact: hosted causal attribution, Windows shared-clock/NLS behavior,
broader GUI/DPI services and full process completion remain separate work.
This fixture change does not establish whole-userland or installed-product
qualification. The earlier helper-corpus compile attempt used a wrong generated
include path and failed before execution; the corrected fresh control produced
the 10,240 successful comparisons above.

Apple documents the clock's tick units in
[mach_absolute_time](https://developer.apple.com/documentation/kernel/1462446-mach_absolute_time).
The timer implementation is owned by
[XNU mk_timer.c](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/mk_timer.c).
The exact conversion and bound above are native measurements and dimensional
analysis, not a guarantee inferred from an unversioned source tree.
