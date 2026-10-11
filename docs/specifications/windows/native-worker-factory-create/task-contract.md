# Worker-factory constructor contract

Acceptance: implement real process-local NtCreateWorkerFactory construction and
close with the measured ten-argument service transport on x86/x64/ARM64; preserve
native/WoW64 capture/probe/handle/access/alias/guard order and owned port/process
references; retain exact ULONG/pointer-width values and executable stack defaults.
No host thread/factory/port is created by guest requests. Scheduling, worker
activation, completion packet/wake and named/security policy remain explicit
unsupported boundaries until their own implementation, never fabricated success.
Run portable/native leaves and original capture replay on all three owning OSes,
C API, affected integrations, all-target builds and five owning consumers. Rebuild
native shipping archive and use identical ordinary observer. Preserve raw failed
query-size attempt and profile compatibility limits. Commit each semantic group
with exact paths and red; publish RAX origin/master plus root origin/master and
19h/master. Full installed-library application support remains the active goal.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
|---|---|---|---|---|---|---|
| W1 | Startup requests ten arguments with max512/reserve1048576 B/commit4096 B | Fresh observer42076 ARM64 X0-X7 and16 B stack | Constructor state/transport | x86 stdcall/x64 spill arguments | Native leaf and stack cleanup differ | Confirmed request, portable three-ABI transports and installed ARM64/WoW64 leaves |
| W2 | Constructor does not immediately create workers | Four native basic captures initially show min/pending/waiting/total0 and may-create1 | Dormant real factory object | zero/all-ones max and hostile entry/stack values | Basic query shows created worker before activation | Confirmed captured constructor paths; scheduling pending |
| W3 | Port/process references belong to factory lifetime | Modeled dependency-close/duplicate/final-close tests; native five-phase internal process-handle query | Automatic dependency pins | Close dependencies before duplicate/final factory close; reference overflow | Dependency disappears/leaks or native lifetime disagrees | Confirmed modeled ownership/native process-handle phases; retained kernel port-reference assumption, biased pointer counts are not exact reference proof |
| W4 | Installed29683 behavior owns current private contract | Original four-profile captures and pinned PHNT declarations | Flags/grants/ordering/state | Wider builds, physical x64 and new rights | Native oracle contradicts current partition | Retained bounded build/host assumption |
| W5 | Zero stack requests use fixed65536 B/4096 B defaults | Native query and corrected actual PE headers at byte72 | Stored zero-request defaults | PE stack1048576 B/4096 B versus2097152 B/32768 B | Factory defaults follow changed PE stack sizes | Confirmed selected ARM64/WoW64 builds; original PE32+ identity collector at byte88 was revised |

Planes: NT export dispatch, private object state/lifetime and wait exhaustiveness
affected. Existing service transports/memory probes reused. Direct decode/execute,
CPU representation, SMIR lift/IR/interpreter/optimizer/lowering/JIT, machine/backend,
static oracle and public C ABI unaffected: no ISA or published layout changes.
C API/Assist process consumers compile the same core; schema, permission scopes,
UI/lifecycle/main-thread dispatch, lanes/prompt, mesh/MCP/crypto/persistence/update
and defaults do not consume private factory fields. Native platform compile and
runtime gates remain mandatory. High remaining scheduling/security/services and
successful applications block full goal. Medium full-run known failures and
physical x64/Linux coverage; low none currently discovered.
