# Fixed StateSeparation registry capture

Windows native-library selection snapshots this fixed registry subtree:

```text
\Registry\Machine\System\CurrentControlSet\Control\StateSeparation\RedirectionMap\Keys
```

The loader's observed `NtOpenKey` uses `DesiredAccess=0x20019`,
`RootDirectory=0`, `Attributes=0x40`, and a counted UTF-16 name of
172 bytes (86 code units x 2 bytes). The shared key-tree model records a
present root with complete bounded descendants, or known absence when the
fixed native root open returns `ERROR_FILE_NOT_FOUND`. Other acquisition
errors propagate. An enumerated child disappearing aborts acquisition.

Guest handles own immutable registry records. Native HKEYs close during
selection. Guest names never enter a host registry operation. Other
StateSeparation keys, the parent keys, and prefix lookalikes retain the
explicit unsupported frontier. IFEO selection uses the same fixed-root model;
its native acquisition and guest behavior remain covered by the existing tests.

Each complete subtree retains the existing limits of 1,024 keys including its
root, depth 32 below the root, 255 UTF-16 units per child component, 32,767 units
per path, and 1 MiB of charged UTF-16 path bytes. All selected keys share the
4,096-value and 16 MiB charged name-plus-data budgets, with at most 1 MiB of
raw data per value. These are payload/metadata accounting bounds, not a claim
that allocator overhead and duplicated map keys consume zero additional memory.
Tree acquisition uses at most three attempts per node with two value/name
samples and matching metadata. It is a bounded node-wise capture, not an atomic
transaction across host registry keys. Guest records become immutable once
selection completes. Excluding native API latency, capture takes
O(B + P + (N + V) L log(N + V)) host work and O(B + P + N + V) retained
space for charged value bytes B, path bytes P, key count N, value count V,
and the maximum UTF-16 comparison length L. The existing 65,536-entry case
table is shared by keys. Guest lookup folds components and traverses ordered
maps, using O(U log N) work and O(U) scratch space for U name units.

## Native evidence

`native-redirection-keys-v1.cpp` and its exact build/run drivers perform only
read-only fixed-key opens. Twelve checker corruption controls cover missing or
duplicated cases, wrong statuses/outputs, handle leakage, producer/source/PE
identity, runtime module changes, and incomplete execution records.

| Profile | Pointer bytes | PE machine | Large-address-aware |
| --- | ---: | --- | --- |
| ARM64 | 8 | 0xAA64 | yes |
| x64 compatibility | 8 | 0x8664 | yes |
| x86 | 4 | 0x014C | no |
| x86 LAA | 4 | 0x014C | yes |

These profiles share one Windows ARM64 kernel; they are not four independent
kernel implementations. The root and tested names are absent on this host.
Every capture returns `STATUS_OBJECT_NAME_NOT_FOUND` (`0xC0000034`) and a zero
output handle; producer process handle counts are unchanged.

The matrix is 4 profiles x 2 samples x 8 name variants x 4 access/view masks =
256 cases. Seven name variants lie at or below the selected root: 224 cases.
The 32 prefix-lookalike observations are an outside-scope native control and
remain unsupported guest lookups. No arbitrary host registry forwarding is
introduced to make those outside-scope observations succeed.

The native regression fails against the unchanged product implementation:
0 passed, 1 failed at the assertion that the independently absent key has a
known-missing guest selection. Its initial compile attempt exhausted guest
disk; that record is preserved separately from the observed regression failure.
Disposable PDB files from older task-owned build targets were inventoried and
removed; no source, executable, static archive, runtime module, or capture was
removed. The retry reached the expected assertion. Present acquisition is
verified through a controlled read-only IFEO source with 54 keys, 106 values,
and 4,756 charged name-plus-data bytes; this does not claim the real RedirectionMap/Keys root is
present. Independent native queries verify every captured descendant and raw
value, and disappearance injection checks acquisition failure.

## Validation

| Host | Complete core: passed / failed / ignored | New focus | C API | Process integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7,976 / 0 / 2 | 6 passed | 183 passed | 544 passed + 0 cfg cases |
| Linux x86-64 container | 7,970 / 0 / 2 | 6 passed | 183 passed | 544 passed + 0 cfg cases |
| Windows ARM64 | 7,419 / 5 / 2 | 9 passed | 183 passed | 0 cfg cases + 4 native passed |

All three all-target build checks passed. The complete core selections are
unfiltered. The exact five retained Windows failure names are recorded in
`validation/windows-existing-failures.json`.

The complete repository input snapshot contains 22,779 files and
1,583,474,670 bytes. The retained Assist consumer checks bind 513 selected
inputs plus native helper identities; this selected set is not the complete
Cargo dependency closure. Actual current Cargo JSON selects the core `.rlib`
and enclosing static carrier with core features `[]`. Full archive-member walks
and hashes are checked before and after the C++ consumers. macOS checks seven
named CTests and the actual Ninja linker commands. Linux and Windows check five
C++ consumers against the exact selected carrier.

These consumers retain Assist Rust ABI 184, with eight C++ inputs compared to
root ABI 264. Publication retains the current root ABI independently. This
record does not establish a whole-current-root GUI, plugin, or package build.
The five Windows lowerer failures are recorded exactly; they do not become
successful tests by being classified as pre-existing.

The smoke program advances from observer turn 2,178,927 to 2,188,223;
`whoami` advances from 2,181,212 to 2,190,508. Both advance 9,296 normally
executed observer turns, return `0xC0000034` with a zero handle at the prior
registry stop, and reach the separate unsupported `NtSetTimer2` service
(`0x1C0`). Neither guest exits successfully. The controller exits zero to
record the observation. Source, programs, observer executable, core archive and
six installed runtime modules are unchanged across the observations. The actual
selected Windows core `.rlib` is 503,963,010 bytes with SHA-256
`790ba7cd4d87fe35b63fdbb4aff228cb0b3996a3a2583d09537139fc029db05e`.

## Provenance and reproduction

The Microsoft API references are stored unchanged beside `primary-sources.json`.
The inherited registry-view contract is documented in
[`shared-registry-keys.md`](../native-session-manager/shared-registry-keys.md)
and [`accessing-an-alternate-registry-view.md`](../native-session-manager/accessing-an-alternate-registry-view.md).
Subkeys inherit the shared HKLM behavior unless a documented redirected parent
intervenes. Native default/32/64 view captures and the native adapter tests
exercise the fixed SYSTEM root.

`check_native.py` verifies the captured producer identities and matrix.
`check_validation.py SOURCE` verifies source identity, full unfiltered test
counts, exact Windows failures, selected archives, and consumer execution.
`validation/` retains source manifests, drivers, command lines, logs, archive
identities, baseline failures, assumption and quality-gate records. The ordinary
observer executes its entire instruction prefix and then reads guest arguments
between normal single-turn slices; it does not skip guest instructions or
substitute native syscall results. Controller exit and guest exit are recorded
separately.
