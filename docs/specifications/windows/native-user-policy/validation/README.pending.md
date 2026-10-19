# Primary-user Safer policy snapshot

Native Windows runtime selection captures exactly one additional registry tree:
`\Registry\User\<primary SID>\Software\Policies\Microsoft\Windows\Safer\CodeIdentifiers`.
The SID comes from the primary-token subject already captured by the same
runtime selection. A guest name never reaches a host registry operation.

The native adapter opens the corresponding `HKEY_USERS` path with query and
enumerate access, copies the complete bounded tree, and closes every native
handle before execution. Only root `ERROR_FILE_NOT_FOUND` establishes absence.
Other root failures, incomplete enumeration, changed metadata, or disappearance
of an enumerated child abort acquisition. Guest keys retain immutable values
and descendants through reference-counted handles. Existing read-only guest
grants and registry error handling are unchanged.

The selected SID and fixed suffix define the lookup boundary. Another SID,
the suffix's parent, and sibling prefixes remain unselected. A second selection
is rejected. The sixth tree shares the existing aggregate limits of 4,096
values and 16 MiB of value-name/data storage. Each complete tree retains the
existing 1,024-key, depth-32, and 1 MiB path-storage bounds.

`user_policy_path` validates revision 1, at most 15 subauthorities, and exact
SID byte length `8 + 4*N`. The 48-bit identifier authority is big-endian;
subauthorities are little-endian `u32`. Native SID text uses decimal authorities
through `0xFFFFFFFF`; larger authorities use `0x` and unpadded uppercase
hexadecimal digits. Subauthorities use unsigned decimal. For the captured
SYSTEM subject, the root is `15 + 8 + 58 = 81` UTF-16 code units, or 162 bytes.
The maximum SID text is 183 code units; the maximum fixed root is
`15 + 183 + 58 = 256` code units, or 512 bytes, excluding a terminator.
SID/path construction takes O(N) time and space with N <= 15. Complete capture,
value aggregation, and ordinal lookup reuse the existing bounded tree code.

The independent C++ probes execute against installed Windows libraries in four
ABI profiles: ARM64, x64 compatibility, x86, and x86 large-address-aware.
These profiles use one ARM64 Windows kernel. Their 1,152 observations comprise
320 absent policy-key opens, 320 controlled-present Environment-key opens, and
512 SID-format observations over 128 distinct SIDs. Source, PE, six native DLL,
exit status, handle balance, and exact observations are bound by
`native-manifest.json` and checked by `check_native.py`. The Environment fixture
exercises native read-only acquisition without creating or changing a key.
The independent Windows adapter test additionally verifies the real primary
policy key across native, 32-bit, and 64-bit registry views and captures the
fixture's three raw values. Twelve rebound logical corruptions are rejected.

Microsoft's stored primary sources describe SID components, the SDK SID-text
conversion, shared registry views, and the `HKEY_CURRENT_USER` to
`HKEY_USERS\[SID]` relation. Native captures determine the recorded SDK's exact
large-authority spelling. Synthetic tests exercise invalid SIDs, SID/suffix
isolation, lifetime, aggregate budgets, raw values, relative opens, read-only
grants, and every guest ABI. The tests-only published baseline fails the real
native absence assertion before the implementation change.

The complete source epoch binds 23,683 files and 1,706,294,072 bytes. Compilation
and runtime results are recorded in `validation/summary.json` and checked by
`check_validation.py`. Retained Assist Rust ABI 184 and 535 selected source
inputs are distinct from a complete build of the current root GUI, plugin, or
packages. Public RAX C ABI 1.14.0, dependency pins, and defaults are unchanged.
The actual retained-owner core is ACTUAL_CORE_BYTES bytes, SHA-256
`ACTUAL_CORE_SHA`.

Both ordinary guest programs pass the previously unsupported policy-key open,
then advance 2,873 observer turns to the FileSystem registry root. Native
controls exit successfully; the emulated guests still stop with an internal
unsupported-call result. Controller exit status zero is not guest success.
The complete same-host process-emulation objective remains active.

The assumption register, change-surface map, bounded findings, exact known
Windows failures, quality gates, original logs, portable checks, and verification
receipt scope are retained under `validation/`. Run the evidence checks with:

```text
python3 -B docs/specifications/windows/native-user-policy/check_native.py
python3 -B docs/specifications/windows/native-user-policy/check_validation.py .
```
