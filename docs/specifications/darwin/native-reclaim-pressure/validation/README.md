# Completed sampled-pressure validation

The source identity is baseba1a0a16 plus three frozen Rust files and one
engineering-document append. The selected manifest binds8,537 pre-run inputs;
new validation records are post-run additions. All canonical core commands use
Rust1.95.0. The owning macOS build independently used1.95.0; the initial isolated
standalone driver resolved to global nightly1.100 and was rerun in a separate,
explicitly pinned target. No global toolchain or repository pin was changed.

| Actual host | Library passed/failed/ignored | Focused | C API |
|---|---|---|---|
| macOS ARM64, Rust1.95 | 7808/0/2 | 6/0 | 168/0 |
| Linux x86-64 container, Rust1.95 | 7802/0/2 | 6/0 | 168/0 |
| Windows ARM64, Rust1.95 | 7228/5/2 | 6/0 | 168/0 |

The five Windows failures are the unchanged four BZHI and one vector FP16
lowerer tests, individually retained and checked. All three all-target checks
pass. Serialized Linux integration passes42 with one ignored on each Unix
host. Windows personality integration passes544 on each Unix host. Windows
native memory integration runs four cases. The pinned native Darwin suite
passes all117 tests,including both workloop-bound cases. Linux-host Darwin
tests are host-gated and do not prove native Darwin behavior.

All three scoped owning builds pass. macOS links CLI/MCP,passes seven CTests
and both protected-text scans. Linux and Windows execute five checked C++
consumers each. Each archive record verifies273 selected RAX source/fixture
identities,empty core features,and complete archive member walks. Nine current
C++ consumers still match the frozen older owning exports. These results do
not establish whole-current-root plugin,GUI or package validation.

The unchanged native reclaim control exits0 and brackets the observed normal
period near10s. The rebuilt Rust1.95 CLI executes the identical control and
also exits0 with a10s bracket at dispatch pressure1. The earlier warning2
native/control pair established1s versus the old model10s. Pure tests cover
warning/critical selection,zero-period accounting,guest clocks,and a closed
profile that never invokes the host query. Native critical pressure,active
swapping overrides,custom tunables and asynchronous guest-ring walking remain
explicit limits under M1; they have not been manufactured to obtain a pass.

Supplementary nightly validation retains a separate workloop finding:
116 passed and1 failed,fixtures::workloop_bound_x86_64. The second dispatch
phase times out with0/4 callbacks. A targeted nightly replay repeats it; the
pinned replay passes. In a valid same-fixture comparison,native execution,
the prechange nightly CLI,and current pinned CLI produce identical output;
current nightly differs despite exiting0. The first comparison's copied file
lacked execute mode and is excluded; v2 preserves0755. The cause remains
unknown. This finding blocks an unconditional nightly/standalone claim and is
being investigated in a separate unpublished diagnostic worktree. No fixture
deadline or assertion was relaxed and no failing record was replaced.

Assumptions M1-M3 remain explicitly reconciled in summary.json. Quality gates
for the pinned sampled-pressure group: normative judgment is not required;
registered assumptions and limits are retained; three-platform source/build
and runtime checks cover the touched contract; ns/tick/percentage arithmetic
is explicit; ignored,failed and unavailable paths stay visible; source,
compiler,producer and raw-output identities are checked. Full process-userland
completion remains unproven: ordinary Windows guest exits are0 and the next
recorded service remains NtSecureConnectPort0x194.

Run check.py and check_validation.py without Python -O. Imported logs/reference
sources retain their original bytes,including CRLF and trailing blank lines.
