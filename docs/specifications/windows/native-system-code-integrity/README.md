# SystemCodeIntegrityInformation native observations

This directory records the pointer-free query contract measured on Windows
10.0.29683. Four executable views ran on one ARM64 kernel: ARM64, x64, x86, and
x86 with `LARGEADDRESSAWARE`. The execution identity was SYSTEM. These results
do not establish behavior on an independent x64 kernel or another Windows build.

Microsoft documents `SYSTEM_CODEINTEGRITY_INFORMATION` as two `ULONG` fields,
`Length` and `CodeIntegrityOptions`; the caller initializes `Length`. See the
[NtQuerySystemInformation reference](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation).
The exact-length, probing, alignment, aliasing, and guard observations below
come from the included native producers rather than that layout description.

## Measured contract

The selected information class is 103. Its external length and embedded
`Length` must both equal 8 bytes. A nonempty output probe requires 4-byte
alignment, including for both x86 views. `ReturnLength` accepts unaligned
addresses. The output probe precedes the optional `ReturnLength` probe; both
precede length dispatch. A zero external length omits the output probe.

Successful queries preserve the first DWORD, copy the full options DWORD,
then write `ReturnLength = 8`. Consequently, aliasing `ReturnLength` with the
options field leaves that field equal to 8. Wrong lengths also report the
required length when both destination probes succeed. Probing faults prevent
the structure copy. Both writable and read-only guards are consumed when their
probe is reached. The probes preserve `LastError` and `LastNtStatus`.

The native options observed here are `0x0000C301`. No meaning is assigned here
to undocumented bits. This query metadata does not demonstrate that the guest
loader enforces signatures, reproduces native Code Integrity decisions, or
supports policy mutation.

## Coverage and results

Cohort 1 has 687 cases per executable view and three samples:
`4 * 687 * 3 = 8244` calls. Cohort 2 adds 80 cases per view and three samples:
`4 * 80 * 3 = 960` calls. The combined total is 9204 calls.

| NTSTATUS | Meaning | Cohort 1 | Cohort 2 | Total |
|---|---|---:|---:|---:|
| `0x00000000` | Success | 84 | 24 | 108 |
| `0xC0000004` | Information length mismatch | 1992 | 144 | 2136 |
| `0x80000002` | Datatype misalignment | 1656 | 0 | 1656 |
| `0xC0000005` | Access violation | 3624 | 336 | 3960 |
| `0x80000001` | Guard page violation | 888 | 456 | 1344 |

The cases include offsets 0, 1, 3, 4, and 7; zero and maximal external lengths;
wrong embedded lengths; null and invalid pointers; read-only, inaccessible,
guarded, and page-crossing output; unaligned, guarded, and aliased
`ReturnLength`. The complete multiplicities are checked by
[project_observations.py](project_observations.py).

## Evidence and reproduction

The two C++ producers are the executed source bytes. Compile each from a
matching Windows SDK compiler environment, using `/std:c++20 /O1 /EHsc /MT`,
with compiler and linker intermediates and `TMP`, `TEMP`, and `TMPDIR` in an
in-tree build directory. For x86 use `/LARGEADDRESSAWARE:NO`; for x86 large
address aware use `/LARGEADDRESSAWARE`. Run each executable with its profile
label as the sole argument. Each invocation emits all three samples.

The observation JSON files contain scalar fields and decoded, initialized
structure DWORDs. They omit raw output buffers and native pointers. The eight
bytes following each readable structure were verified unchanged. Original
logs remain private; their byte lengths and SHA-256 identities, the original
receipt identities, executable identities, selected native module identities,
and execution-identity hash are in [provenance.json](provenance.json).

Given the original private bundle directories, the projection is reproducible:

```text
python project_observations.py --input-root <bundle-parent> --output <fresh-output>
```

[projection-controls.json](projection-controls.json) records three accepted
baselines and 48 rejected alterations under normal Python, `-O`, and `-OO`.
The controls cover source/log identities, completion, profiles, executable and
runtime stability, execution identity, case multiplicity, errors, status,
readability, and output extent. They validate the projection, not an
independent native execution.

## Implementation boundary

The selected native runtime captures only this fixed query into an exclusive
8-byte structure and a separate `ULONG` return length. It accepts two agreeing
captures, with at most three pairs. Guest pointers and lengths never enter the
host call. Capture failure remains an explicit unavailable property; it does
not substitute zero options or disable unrelated native inputs.

Guest queries use guest-memory probes and the captured full DWORD. Synthetic
tests cover all three guest ABIs, zero and maximal options, copy/alias order,
guard consumption, maximal length, and unavailable metadata. Native probe
success is distinct from the emulated program completing its bootstrap.

## Assumption register and bounded scope

| ID | Assumption | Basis and dependent result | Stress test and falsification probe | Status |
|---|---|---|---|---|
| CI1 | The selected host's agreeing query captures supply initial guest query metadata. | Fixed native acquisition supplies `NativeRuntime.code_integrity`. | Change native policy between captures; disagreement rejects acquisition. A later guest policy change would require a separate state contract. | Retained |
| CI2 | Boundary observations apply to the measured build and kernel. | Both complete native cohorts agree across the four executable views. | Run the included producers on another build or an independent x64 kernel and compare statuses, writes, and guards. | Confirmed within the stated scope |

High impact: signature enforcement and policy mutation remain separate,
unimplemented contracts; this evidence does not establish complete Windows
policy emulation. Medium impact: independent x64-kernel and other-build
coverage is unknown. Neither limitation changes the recorded measured query
results.
