# Native Windows clock fault profiles

The installed x64 `NtQuerySystemTime` export on build 20348 jumps to a user
routine that reads `KUSER_SHARED_DATA.SystemTime` and stores it through the
caller pointer. Build 29683's selected ARM64, x64 and x86 exports execute system
calls. Their output-pointer validation cannot be assumed identical. This control
measures the native contract before changing the runtime or installed-leaf tests.

Run on an x64 Windows kernel with Visual Studio C++ tools installed:

```text
python docs/specifications/windows/native-clock-profile/run-v3.py
```

The default architectures are x64 and x86. A native ARM64 host can additionally
select `--architectures arm64 x64 x86`. The optional `--environment` names an
existing private compiler-environment snapshot; its values are not logged.
Both routes select the installed developer command and validate the actual
compiler output's PE machine before execution. Compiler temporaries, objects,
executables and intermediate directories remain inside this worktree and are
removed after each completed architecture. Source, compiler and pre/post-run
executable hashes bind each observation. The source digest is rechecked at exit.

`native-clock-profile-v1.cpp` runs 16 bounded own-process cases. Each owns six
4,096-byte scratch pages, initialized to `0xA5`, with ordinary writable,
readonly, inaccessible and guarded regions. It tests aligned/unaligned output,
NULL, address one, the top pointer value, readonly/noaccess output, split stores
and guard/alignment priority. Every scratch allocation is explicitly released.
Only ordinary writable pages are read after the query, so observation cannot
consume a guard. Reported writes and fault addresses are scratch-relative.

Each case distinguishes a returned NTSTATUS from a caller SEH exception and
records guard protection, changed bytes, last-error and last-status. Valid
native clock samples bracket each call in FILETIME ticks of 100 ns since
1601-01-01 UTC. These are native NT-clock samples, not the precise Win32 clock.
The producer also reports the selected NTDLL's PE machine, export RVA and first
48 bytes. No host address, token, process memory outside these fixed inputs,
guest operand or callback is emitted or forwarded to a native service.

The driver validates complete captures and successful producer cleanup; it does
not assert that different Windows builds must return identical service statuses.
`windows-clock-profile.yml` runs the control independently on Windows 2022 and
2025 x64 runners, including their WoW64 views. It needs no Rust compilation, so
native measurements remain available separately from emulator regression jobs.

## Recorded local qualification

The first controller failed before compilation: Python's argument-vector
quoting inserted literal escaped quotes into the developer-command path. Its
source and failed record remain preserved. A fresh version passes the exact
Windows command line directly to `cmd.exe`, disables AutoRun/delayed expansion,
keeps captured environment output private and completes all three architectures.
The developer command changes only its child process environment.
The next hosted attempt exposed a case-sensitive dictionary lookup after copying
Windows environment variables. Version 3 normalizes inherited and snapshot keys
before selecting `PROGRAMFILES(X86)` or the compiler directory. Version 2 is
retained as the executed historical producer; the workflow selects version 3.

All three views run on one ARM64 kernel, build 29683. They are not independent
x64-kernel measurements. Each has 16 returned calls, zero caller exceptions,
unchanged seeded last-error and unchanged sampled last-status. Representative
statuses are identical in those three views:

| Cases | NTSTATUS | Ordinary writable output | Guard effect |
| --- | --- | --- | --- |
| Aligned 8/4-byte outputs and last valid 8 bytes | `00000000` | Clock value written | None |
| Unaligned 1/2/3, address one, top value, split readonly at +4095 | `80000002` | Unchanged | None |
| NULL, readonly, noaccess, split readonly at +4092 | `C0000005` | Unchanged | None |
| Guarded output and four-byte split into guard | `80000001` | Unchanged | Selected guard clears to `PAGE_READWRITE` |
| Unaligned guarded output | `80000002` | Unchanged | Guard retained |

These 48 calls are native observations, not 48 emulator regression passes.
The captured x64-kernel build-20348 jump/body and zero guest output already
explain the missing shared-time input. Native older-build fault/guard controls
are still required before selecting its runtime/test profile. No production
clock adapter, service decoder, profile fence or tolerance changes here.

The producer inspects a fixed 8,192 ordinary scratch bytes per case, with at most
16 cases per architecture; time and space are bounded constants. Compiler and
OS query costs are external. Public output is bounded by the fixed producer;
the driver rejects captures exceeding 262,144 characters.

## Assumptions and boundaries

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| CLKF1 | A direct user-mode clock store has different fault/alignment semantics from a kernel query. | Captured build-20348 x64 body versus build-29683 syscall leaves. | Future clock profile and installed-test expectations. | Unaligned, readonly, cross-page and guarded outputs on both kernels. | Compare actual returned-versus-SEH records and partial writes. | Retained pending hosted x64 measurements |
| CLKF2 | Local ARM64/x64/x86 instruction views establish independent kernel parity. | All report build 29683 on one ARM64 VM. | Independent x64-kernel claims. | Hosted Windows 2022/2025 x64 controls. | Compare actual kernel builds, selected images and outcomes. | Falsified; local scope is one kernel |

High impact: native valid-call success alone does not qualify NLS fault/alias
semantics or admit another NLS profile. Clock refresh, older-profile handling,
DPI/GUI work and full userland completion remain unfinished. The Windows native
producer fundamentally requires Windows NT and MSVC SEH; the driver rejects
other operating systems. Shipping Rust/C++ targets and their existing three-host
source inputs are unchanged by this control. No platform skip or availability
stub is added. Before publication, the control was replayed over 14 unrelated
ISA/HVF commits ending at `0106a4d5336478a25b274057029292c3ec08e63c`.
Its native producer/driver bytes remain identical; those observations do not
qualify the combined Rust/product frame. Earlier compiled/runtime evidence
retains its original revision, and the new parent requires separate gates.

Microsoft documents the function's FILETIME units and NTSTATUS interface in
[NtQuerySystemTime](https://learn.microsoft.com/en-us/windows/desktop/api/Winternl/nf-winternl-ntquerysystemtime)
and the guard's one-shot behavior in
[Creating Guard Pages](https://learn.microsoft.com/windows/win32/memory/creating-guard-pages).
The precise alignment, exception and partial-write results here are native
observations bound to the selected build and instructions.
