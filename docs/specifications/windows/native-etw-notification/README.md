# Installed Windows ETW process notification Event

Captured 2026-10-12 on Windows ARM64 build10.0.29683.1000, with native ARM64,
x64 compatibility, WoW64 x86, and x86/LARGEADDRESSAWARE producers compiled by
Visual Studio18.7.1 with the static CRT. This is a measured private function27
contract, not a general ETW implementation.

PHNT `ntwmi.h`, commit53fbbdc5b5d2b08761db1c7b26bfa8c820924356,
provides the six-argument `NtTraceControl` declaration and class27 name
`EtwAddNotificationEvent`. Its `in HANDLE` annotation does not establish the
private input width: all four measured callers require exactly one4 B ULONG.
Canonical source:
https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntwmi.h
The retained original header and license preserve primary-source provenance.
Header SHA256 ea487f6366b9678ab31a2f8509b4fbc0be63d4deb83a86b123da83c66b21cf21.

Each mature caller supplies480 Event/type/access/length cases,4200 pointer,
span, alignment, protection and guard cases, and1050 read-only output cases.
Fresh ARM64 owned children provide48 first/repeat/close/replacement sequences,
120 ReturnLength-fault sequences, and72 overlap/output-alias sequences, with
one excluded debugger warmup per family. Total:4*(480+4200+1050)+48+120+72
=23160 complete cases;3 debugger warmups excluded. Hooked installed entry and
true return are verified against actual native bytes and MEM_IMAGE ownership.
The producer redirects only these diagnostic child calls into owned scratch,
restores breakpoints, terminates each owned child with77, and drains exit events.
These are independent native kernel oracles. The ordinary process observer does
not redirect guest calls and is separately retained before/after the change.

Output non-NULL/length>0 receives a full write probe first. Required ReturnLength
is non-NULL and receives a4 B write probe, then exact input length/non-NULL input
validation precedes DWORD handle capture. Byte-aligned input/output operands
are accepted. NULL optional output is ignored even with a nonzero length.
Type and EVENT_MODIFY_STATE right0x2 precede busy statusC0000718. NULL handle
value isC000000D; garbage and zero-extended pseudo-handle bit patterns are
C0000008, SemaphoreC0000024, insufficient Event grantsC0000022. Success writes
zero4 B ReturnLength, failures leave it unchanged. Guard probes consume only
the first reached guard. First successful registration survives caller close
and blocks replacement, without changing Event type/signal state. Biased native
ObjectPointerCount values do not prove an exact host reference count.

The model retains a separate private process Event reference from WNF and
releases it through object-table drain. It uses no host ETW operations. Other
TraceControl functions remain explicit unsupported boundaries. Fresh x64/WoW64
first-publication/lifetime is a retained same-kernel assumption (A1), supported
by identical mature validation rows but not a fresh compatibility debugger
capture. A2 bounds the private contract to the measured installed build.
See assumption-register.json and change-surface.json for falsification probes
and ownership. Public CAPI1.11.0/Assist ABI184 remain unchanged.

The canonical observer baseline reaches unimplemented NtTraceControl0x1D6 at
turn50254, thread8, PC0x180002DA0, function27, input0xABE800 (DWORD0x3C),
input length4 B, NULL output/0 B, ReturnLength0xABE804. Full ordinary guest
startup and process exit are separate acceptance criteria; this service alone
does not establish them. Final host/product evidence and exact source/archive
identities are recorded by the manifest and validation summary.

Validation for this increment:

| Host | Focused passed | Full passed/failed/ignored | C API | Registered integrations | Owning C++ |
|---|---:|---|---:|---:|---:|
| macOS ARM64 |8 |7681/1/2 |168 |544 |5 |
| Linux amd64 CPU translation/native kernel |8 |7675/1/2 |168 |544 |5 |
| Windows ARM64 |9 |7087/5/2 |168 |4 |5 |

The original complete suite outputs are retained. The macOS linked-timeout and
Linux update/remove-timeout failures occur in unchanged Linux-personality source;
each exact isolated test passes. The five Windows BZHI/FP16 failures equal the
prior published failure set. All new ETW and prior Event/subscription tests pass.
The installed ARM64/WoW64 leaves execute real syscall/return instructions;
x86 RET24 consumes24 B arguments plus4 B return address, for28 B total.
Owning macOS CLI/MCP, seven selected CTests and both protected-text scans pass.
The five C++ consumers run on each host against the new owning archive.

Windows static-CRT Assist/core archives are1119868104 B/732580332 B with4673/260
structurally walked members. Hashes and61 source units/two actual helpers are
bound in owning-source-hashes.json and native-etw-notification-owning-archive-hashes.json;
core features remain empty. The native overlay applies17477 source files and234
patches with zero mismatch, preserving the374 B offline adapter. The Unix
owning root e2fc5b48 differs from compiled root814518f7 only in the two engineering
records and gitlink, with no owning code/config/resource delta.

The identical11177 B ordinary observer advances50254->50306:50306-50254=52
scheduler calls. It now stops at NtTraceControl function15/EtwRegisterGuids,
SVC0x1D6, thread8, PC0x180002DA0, arguments[15,0xABE880,160,0xABE880,160,0xABE870].
All four ordinary programs still report reason5/exit field0/internal failure
before guest exit. No guest startup parameters, code, callbacks or memory are
rewritten by that observer. The additional input observer only reads guest data.

Assumption A1 remains retained; fresh compatibility publication/lifetime is
unknown. A2 is confirmed for the measured build only. Quality gates cover the
bounded function27 increment, exact source ownership, primary/native provenance,
probe/alias/lifecycle edges, units, and recorded high/medium scope limits.
Complete ordinary process execution and later TraceControl services remain work
for the active full-process goal; this evidence does not claim completion.

The first parallel Windows C API run passed166/failed2 due to process deadlines
(reason6 timeout instead of expected reason4 completion); both process-test
sources are byte-identical to the parent. The complete serial retry passes168/0,
with no changed assertions or skipped cases. Original stdout and gate JSON are
retained as diagnostic-windows-capi-parallel.log and
diagnostic-windows-gates-before-capi-retry.json. Remaining-gates/recovery JSON
records the actual serial command and subsequent build/integration commands.
The completed Windows owning job returned0. Temporary Parallels command/UI
stalls resolved; no VM reset or task client termination was performed.
