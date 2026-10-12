# Native WNF process notification Event registration

Baseline RAXa157e0ee/root77df475c; build29683.1000 Windows ARM64. The service uses one HANDLE, validates existence/type then EVENT_MODIFY_STATE0x2, and retains one process-owned Event identity on first success. A repeated valid request returns STATUS_ALREADY_REGISTERED0xC0000718. NULL is invalid and cannot clear registration. Last caller-handle close does not permit replacement. Type/signal state remain unchanged; object-table drain removes the registration and retained object. Other WNF state/subscription/delivery APIs remain unsupported.

| Native source | Successful row/sequence count | Scope |
|---|---|---|
| Initialized ARM64/x64/x86/x86-LAA producers |4*48=192 | Type/access before busy; NULL never clears |
| Fresh debug-only ARM64 children |48 plus1 explicit warmup | First/repeat, selected-handle close, replacement/repeat |
| Installed ARM64 RTL translation |5 | Already-registered status maps to1359 ERROR_INTERNAL_ERROR |
| Registration replay total |240 | Failed compatibility captures excluded |

Six native producers have actual compiled source/image/log/DLL/PE identities in native-wnf-process-event-producer-identities.json. PHNT ntexapi.h at pinned53fbbdc5b5d2b08761db1c7b26bfa8c820924356 and its license are retained; primary-contract.md identifies the exact one-HANDLE declaration and Microsoft debugger sources. Fresh children resume the original installed syscall leaf, restore patched return bytes and terminate with task-owned exit77 before continuing normal RTL. The first debugger child initializes persistent DebugObject/ETW/key handles; all48 subsequent ARM64 cases preserve exact parent handle sets. NtQueryObject PointerCount is biased/variable and is not exact pin proof.

Fresh x86 debugging reaches the setter but fails after a patched return site/replay. x64 startup bypasses the intercepted exported leaf; an explicit main call is already registered, and its partial sequence later times out. Every diagnostic source/driver/raw log is retained with diagnostic- prefixes. They do not count as completed fresh matrices. AssumptionA3 explicitly extrapolates ARM64 first-state semantics to same-kernel compatibility callers; native mature profiles, synthetic all-ABI tests and actual installed native/WoW64 guest leaves stress that assumption. Physical x64 Windows remains unproved.

| Host | Focused passed | Full passed/failed/ignored | C API | Integrations | Owning C++ |
|---|---|---|---|---|---|
| macOS |7 |7660/0/2 |168 |544 |5 |
| Linux |7 |7652/2/2 |168 |544 |5 |
| Windows |8 |7063/5/2 |168 |4 |5 |

The observed baseline is0 passed/5 failed: three explicit unsupported-service failures and two synthetic service-table fixtures lacking the service prototype. Final registration tests cover aliases/final close, handle reuse with no replacement pin, both event types and preserved signal states, denied/wrong-type/null validation before busy, per-process isolation, one formal argument without buffer probes, process object drain and transactional reference-count overflow. Native leaves additionally prove x86 RET4/8 B total caller cleanup. Actual RTL translation is replayed independently.

All15 gate selections per host are retained; full Linux and Windows failures are the previous io_uring timing/BZHI/FP16 cases, without weakened assertions/skips. Both Linux timeout isolates pass. Unix C API and Linux integrations are serial; Linux amd64 CPU execution is translated while the owning kernel is Linux. macOS CLI/MCP, seven CTests and both protected-text scans pass. Final formatting changes only independent module declaration order and one constant's documentation; final-source-formatting-proof.json binds before/after bytes, final targeted/all-target rebuilds and refreshed owning artifacts.

Native final-source/source-overlay/archive identities are in validation-summary.json, owning-source-hashes.json and native-wnf-process-event-owning-archive-hashes.json. All47 source identities, two helper identities, empty core features and complete archive member walks are required by the strict checker. The unchanged374 B offline adapter is distinguished from canonical .cargo/config.toml. Unix frozen root e2fc5b48 differs from compiled root77df475c only in two engineering records/gitlink; owning code/config/resource bytes match. Opaque C API1.11.0/Assist Rust ABI184, Object/Proc public shapes and package layout are unchanged.

The identical11177 B observer reaches NtSubscribeWnfStateChange0x1CD/PC0x180002D10 on thread8, with formal arguments[0x11037C0,0,0x11,0xABE918]. 46245-44258=1987 scheduler calls, not instructions. All four smoke/MSVCRT/UCRT/installed-whoami programs still report reason5/exit_code0/internal failure before guest exit. No success/exit/notification is manufactured.

SCOPE.md contains acceptance, assumption register with falsification probes, complete execution/product plane mapping, complexity/units and bounded high/medium findings. Strict validation uses explicit exceptions and refuses optimized Python:

```
python3 docs/specifications/windows/native-wnf-process-event/check_native_wnf_process_event.py
```

The full emulation goal remains active. Notification state/subscription/delivery, worker activation/completion and complete ordinary program execution remain unfinished. Package/live IDA/Qt and alternate native profiles remain unproved.

| Final native static-CRT archive | Bytes | Members |
|---|---|---|
| Assist |1111673464 |4673 |
| Empty-feature core |724852772 |260 |

Their full SHA-256 values and47 actual compiler source hashes/two helper hashes are retained in the owning archive record.
