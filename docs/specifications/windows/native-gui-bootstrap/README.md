# Native GUI conversion and callback continuations

The candidate implements the measured `NtGdiInit2` process/thread conversion
mechanics and a thread-owned continuation for callback 84. The selected
installed USER32 callback executes as guest instructions. Kernel initialization
does not fabricate that callback's PEB/TEB writes.

```text
NtGdiInit2
    -> private guest copies of selected GDI/desktop views
    -> kernel-owned process/thread fields and per-thread seed
    -> selected NTDLL dispatcher -> guest USER32 callback 84
    -> NtCallbackReturn -> saved service context and process value, or zero
```

Native ARM64 observations provide a zero-byte input and a callback frame whose
input pointer is SP + 48 and interrupted SP is SP + 64. The frame retains the
saved FP/LR and service resume address. The model preserves the interrupted
register context and HLE depth. The observed caller consumes the callback status;
its return-buffer pointer and length are not dereferenced. Native negative
status returns zero from conversion. A call without an active callback returns
`0xC0000258`, preserving last-error, across 80 native controls.

The selected GDI view is 385 * 4,096 = 1,576,960 bytes. The desktop allocation
contains a 106,496-byte committed prefix and a 684,032-byte reserved tail:
106,496 + 684,032 = 790,528 bytes. Each guest thread receives its own 32-byte
observed seed, with a guest clock word at offset 20. This seed is not a claimed
ABI extent. Thread retirement releases its bookkeeping for reuse.

Copied views retain their captured virtual addresses so their references name
private guest mappings. Guest pointers never enter a host query or callback.
Kernel-only growth and writes require an allocation-identity token and the
expected read-only/reserved profile. A stale token cannot write, grow or retire
a replacement at the same address. Ordinary guest allocation cannot acquire the
token. Native metadata acquisition uses an owned host helper thread and bounded
repeated captures; non-GUI behavior retains its separate availability path.

`observations.json` projects eight native ARM64 callback-control executions,
including controlled failure, nested conversion and invalid output arguments.
The projection retains relative frame fields, selected scalar changes and
statuses. Original stack, TEB, desktop and GDI buffers remain private and are
bound by their original log hashes. Controller 5 compiled source 4; controller
6 compiled source 5. Original receipts and source hashes define attribution.

`inputs.json` binds the published projection and producer files. The portable
projection script accepts an input directory containing the original private
logs and writes a new output directory. Its integrity controls accept three
unmodified runs and reject 24 corruptions across normal, `-O` and `-OO` Python
execution; the original control record is `projection-controls-v1.json`.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| GUI-A1 | Captured read-only metadata suffices for the measured conversion callback | Independent view geometry, native callback controls, and installed USER32 execution | Selected guest input views | Repeated capture, actual guest callback, concurrent desktop changes | Incoherent capture or required absent metadata | Confirmed for the measured callback; broader GUI behavior remains incomplete |
| GUI-A2 | The observed 32 bytes suffice as the current initial seed | Zero/pointer-free initial record with clock word | Per-thread seed allocation | Independent threads and actual guest callback | A required initial field outside that range | Retained; ABI extent unknown |
| GUI-A3 | The client version follows the measured PE subsystem conversion | Independent header controls and minor-byte boundary cases | Guest-derived expected-version field | Main/worker and high minor values | A differing native word | Confirmed for recorded cases |

| Impact | Remaining boundary | Effect |
| --- | --- | --- |
| High | Broader USER/GDI services, including the raw process DPI setter | Full GUI and process objectives remain incomplete |
| High | WoW64 paired native process/thread contexts | x86 GUI conversion remains explicitly unavailable |
| High | Guest MUI cache clear/commit | Full MUI capability remains incomplete |
| Medium | Independent x64 kernel | x64 frame implementation lacks independent kernel runtime proof |

Shared CPU tests execute synthetic ARM64 and x64 dispatcher/callback/return
instructions and check continuation restoration. Separately, the selected
installed ARM64 USER32 executes callback 84 as guest instructions. The fine
observer sees `NtCallbackReturn` and verifies restored PC and SP, a nonzero
service result, and agreement between its low DWORD and the guest PEB value.
The guest callback performs all five observed USER32 PEB/TEB writes.

On the integrated candidate, callback entry occurs at observer turn 2875 and
return restoration at turn 498987. The next unimplemented service is
`NtUserSetProcessDpiAwarenessContext` at turn 498994, after the callback has
returned. These are observer turns, not retired-instruction counts. Earlier
observers switched to larger slices before the return and therefore could not
establish its absence. Their incomplete observations are retained; they do not
support a stuck-callback conclusion.

The ordinary smoke image exits successfully. The ordinary `whoami` image
remains incomplete at the DPI service. Callback restoration does not establish
whole-program completion. Retained Assist ABI184 archive/consumer checks pass
on macOS, Linux, and Windows; those checks are distinct from current-root
ABI275 GUI/plugin/package validation. [validation-current.json](validation-current.json)
records the tested source parent, exact evidence identities, and remaining
validation boundaries.

At the original GUI candidate, before the AArch64 capability-test correction,
the unfiltered Windows suite reports 7757 passes, five established ARM64
lowerer failures, and two ignored tests; that frame is not a passing full suite. The
actual retained C++ adapter observes the expected USER32 fields in the retained
`FAILED` process at the DPI frontier. A separate fine-sampling C API observer
exited with `0xC0000409`; its cause is unknown. That failed observer is preserved
and does not establish a callback failure or a successful whole-program run.

The subsequent `7893d21c` capability-test correction passes the unfiltered native
ARM64 library suite: 7763 passed, zero failed, two ignored and zero filtered.
The hosted x64 build-20348 process slice is a separate frame, with 715 passed
and six failed. Its host identity/build/session assumptions and explicit missing
Win32u capability are addressed by the
[installed-profile controls](../native-installed-profiles/README.md).
That record preserves the unresolved hosted clock failure and the distinction
between unavailable capability checks, actual installed leaves and full GUI
support.
