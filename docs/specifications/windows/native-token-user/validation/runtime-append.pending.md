

## Native TokenUser identity (2026-10-19)

NtQueryInformationToken class1 publishes captured primary or modeled
impersonation SID/User.Attributes without host handle forwarding. Native
acquisition retains opaque attributes; existing ACL interpretation is unchanged.
Native64 full-span/alignment/length probes, typed/query handles, untouched
padding and required-length-before-data differ from the measured WoW64
scratch/copy/header/optional-length-last phases. Direct faults preserve reached
copy progress and guard state. WoW64 scratch failure sets LastError8 while
preserving last-NT-status, with no guest-sized host allocation.

The independent native corpus binds22688calls across four ABI profiles on one
ARM64 kernel;12corrupted copies are rejected. Thirty-six additional native TEB
controls preserve last-NT-status. Five tests compile and fail before changes;
final five shared and six native Windows tests pass, including installed
host/WoW64 leaves versus independent native SID/attribute acquisition. Complete
unfiltered core pass/fail/ignore is macOS8013/0/2,Linux8007/0/2,Windows7467/5/2,
retaining the exact five established Windows lowerer failures. CAPI183,
integration and all-target checks pass on all three hosts. Actual retained
Assist core/carrier archives and C++ consumers bind530selected source inputs
with complete archive walks before/after links. Retained Rust ABI184 and the
prior eight-C++-input comparison to root ABI264 do not establish whole-current-
root GUI/plugin/package coverage. Public RAX C API remains1.14.0.

Both ordinary Windows guests return TokenUser success and advance2743normally
executed observer turns to an uncaptured per-user Safer registry key, without
successful guest exit. Native controls exit0 with stable output. The exact
validated owning core is ACTUAL_CORE_BYTES bytes, SHA256
`ACTUAL_CORE_SHA`. The pre-gate source epoch binds23442inputs/1695720818bytes;
this engineering append is separately bound. Current-SYSTEM-SID native scope,
other-SID copy inference, existing guest scratch-budget limits and complete
commands/data/assumptions are recorded under
docs/specifications/windows/native-token-user/.
