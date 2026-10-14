# Mapping integration after upstream AArch64 expansion

The unpublished mapping commit was rebased onto upstream
`0705e87135c6f6265e9ec618704041c3febcdf7e` on 2026-10-14. Its rebased identity is
`a40ea6ba16a92d4cc5758836c1543d0843f63cfb`. The upstream changed 196 files,
including 77 Rust files, and added one test in the selected configurations.
There was no intersection with the mapping increment's 247 owned paths. Rebase
verification preserved all 247 original blobs. No merge commit or published
history rewrite was used.

This directory records combined-source validation. The parent directory retains
the preceding validation against `9b7244e7186938383a65c06d26b94bbdcaeddb9e`;
its counts and archives keep that historical scope.

| Gate | macOS | Linux x86-64 | Windows ARM64 |
|---|---|---|---|
| Focused mapping tests | 4 passed | 4 passed | 5 passed |
| Full library | 7764 passed, 2 ignored | 7758 passed, 2 ignored | 7179 passed, 5 baseline failures, 2 ignored |
| C API | 168 passed | 168 passed | 168 passed |
| All targets | Passed | Passed | Passed |
| Windows portable integration | 544 passed | 544 passed | Host-selected 0 tests |
| Windows memory integration | Host-selected 0 tests | Host-selected 0 tests | 4 passed |
| Scoped owning C++ consumers | 5 passed plus 2 manifest CTests | 5 passed | 5 passed |

The five Windows failure identities exactly match the preceding capture.
Ignored and host-elided tests do not establish runtime behavior. The Windows
focused gate includes actual ARM64/WoW64 installed leaves and the Rust AMD64
projection-cache assertion. The synthetic replay covers 4236 calls on all three
hosts. Native producer evidence remains the distinct 4632-observation corpus.

Owning archive member walks, current source hashes, Cargo selection and nine
current C++ consumer hashes are retained. The 201 scoped Rust files comprise
124 Windows source units and the 77 changed upstream Rust files. The separately
verified 196-file native overlay also binds Cargo.toml and the remaining
upstream files. This is scoped affected-source evidence, not a claim that every
repository source file was compiled. All core default features remain disabled.
Frozen Assist roots remain e2fc5b48 on Unix and f386ed2 on Windows. Full current
root, plugin, GUI, packages and deployment are excluded. C API 1.11.0 and all
existing ABI/default-feature contracts are unchanged.

The unchanged 11177-byte observer is compiled against the same verified RAX rlib
carried by the Windows Assist archive. It again advances161784 ->168227 turns,
delta6443, and stops at NtQueryPerformanceCounter0x31/RVA0x1350. Three CRT
programs share that frontier; whoami retains NtTraceEvent0x5E/RVA0x1620. All four
ordinary runs remain Internal; zero guest exits are proved. The complete
same-host three-platform process goal remains active.

```sh
python3 -B docs/specifications/windows/native-section-map/verify-evidence.py
python3 -B docs/specifications/windows/native-section-map/upstream-integration/check-integrated-validation.py
```

[validation-summary.json](validation-summary.json) binds the results and
archive identities. The checker rejects optimized Python, verifies exact counts
and baseline failures, checks every recorded scoped source and upstream input,
checks complete archive walks and empty core features, and verifies actual
ordinary observer/continuation output. Native binary identity was captured by
the owning Windows inspector; binaries are not redistributed.

The mapping Assumption Register remains applicable because all changed mapping
Rust files are byte-identical. Upstream CPU effects were treated as unproved
until combined gates and ordinary execution completed. No remaining CPU
integration contradiction was observed in this measured scope. Mixed-machine
execution, other native Windows kernels and full ordinary completion remain
high-impact limits recorded in the parent contract. Root GitLab connectivity
was unavailable during preparation; remote publication is verified separately
from source/runtime validation and is not inferred from these tests.
