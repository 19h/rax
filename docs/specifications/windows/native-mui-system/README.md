# Fixed machine MUI language configuration

The selected native registry snapshot includes the complete fixed tree at
`\Registry\Machine\System\CurrentControlSet\Control\MUI`. The ordinary
installed USER32/RTL path requested its `Settings` child after the selected MUI
query data and language-generation counter had become coherent.

Acquisition uses the existing read-only tree capture. The root is a compile-time
constant. Guest paths do not enter a host registry call, and guest key handles
retain immutable snapshot records rather than host handles. Existing limits
remain 1,024 keys, depth 32, 1 MiB of path bytes, 4,096 values and the combined
16 MiB registry data budget. A native absence result is selected only for
`ERROR_FILE_NOT_FOUND`; other acquisition errors propagate.

## Native controls

Both controls ran on Windows ARM64 kernel 10.0.29683.1000. ARM64, x64, x86 and
large-address-aware x86 are executable views of that kernel. Each control
captures the three registry views twice in each executable, yielding
4 * 3 * 2 = 24 complete captures. Contents and ordering agree within each
control, including key timestamps and raw value data.

| Fixed root | Keys | Values | Value names and data | Path bytes |
| --- | ---: | ---: | ---: | ---: |
| Machine MUI `Settings` child, v3 | 2 | 1 | 54 bytes | 296 bytes |
| Complete machine MUI tree, v4 | 17 | 22 | 866 bytes | 2,900 bytes |

Each executable also performs 144 native key-open controls. Of these, 128 open
successfully and are closed; the process handle count agrees before and after.
PE machine values and large-address-aware flags are independently decoded from
the same executable bytes whose hashes agree before and after execution.
Original receipts bind the compiler, source, kernel image and native NTDLL,
KernelBase and Advapi32 bytes. The complete input identities and normalized
capture identities are in `inputs.json`.

Shared tests cover exact-root selection, case and trailing-separator handling,
unselected siblings and parents, and retained raw values after the snapshot is
dropped. Native adapter tests compare the selected tree recursively with an
independent native open/enumeration in all three registry views, and inject
non-absence errors to check their priority. Candidate validation and ordinary
guest callback completion remain pending for this addition.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| MUI-R1 | A fixed immutable machine MUI tree represents the current language-configuration input | Measured guest request for `Settings` and 24 identical full native captures | Read-only snapshot selection | Independent native views, exact-root boundaries and capture budgets | Divergent captures, a selected parent/sibling, or guest path forwarding | Retained; candidate validation pending |

| Impact | Remaining boundary | Effect |
| --- | --- | --- |
| High | Ordinary callback completion and owning consumers | These native metadata controls alone do not establish either |
| Medium | Independent x64 kernel and other Windows builds | Additional native profiles remain unknown |
