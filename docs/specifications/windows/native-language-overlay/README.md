# Fixed language-overlay package metadata

The native snapshot selects the complete fixed registry tree at
`\Registry\Machine\Software\Microsoft\LanguageOverlay\OverlayPackages`.
The installed USER32/RTL path requested its `en-US` child after querying the
selected MUI data and machine MUI configuration. Language names are not
forwarded from guest calls to host registry APIs.

The root and the measured child were both absent on the observation host.
Each of the two controls captures three registry views twice in four executable
views, for 2 * 3 * 2 * 4 = 48 captures. Each executable also performs 144
`NtOpenKey` controls: 2 * 4 * 144 = 1,152 calls. Every recorded call returns
`STATUS_OBJECT_NAME_NOT_FOUND` (`0xC0000034`), preserves last-error
`0x12345678`, and produces no handle. Handle counts agree before and after.
The `TREE_END` key counter includes an unsuccessful root attempt; its value 1
does not indicate that a key exists.

The four executable views are ARM64, x64, x86 and large-address-aware x86 on one
Windows ARM64 kernel 10.0.29683.1000. Independent native x64-kernel observations
remain unknown. The original receipts bind source, compiler, kernel image and
the native NTDLL, KernelBase and Advapi32 files. `inputs.json` binds the original
logs and producer files byte for byte.

The adapter uses the existing bounded read-only tree capture. Only native
`ERROR_FILE_NOT_FOUND` selects an absent tree; other errors propagate. A present
tree retains complete raw values and descendants within the existing budgets.
Shared tests cover exact-root selection, case, trailing separators, parent and
sibling rejection, and retained key/value lifetime. Native adapter tests
compare all three registry views independently and test non-absence errors.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| LO-A1 | Fixed-tree absence supplies the measured child-open result | Native root and exact `en-US` controls return `0xC0000034` | Selected snapshot lookup | Three registry views, four executable views, exact-root boundaries | Different child status or unintended selection outside the root | Retained; candidate native/runtime validation pending |

This metadata addition does not establish completion of the USER32 callback,
MUI cache mutation, all GDI services or the complete process-emulation goal.
