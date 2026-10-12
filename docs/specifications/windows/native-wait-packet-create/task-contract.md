# Wait completion packet contract and initial assumption register

Initial baseline root7e55629b/RAX640a7f0d. Concurrent root changes advanced through
d7427260 to e2fc5b48 and were preserved. Owning validation uses committed e2fc5b48;
uncommitted user edits were excluded from the native source overlay.
Acceptance: real guest packet constructor/resource/access/attribute/fault/guard/alias
semantics and native transport, immediate/repeated wait behavior and duplicate/final
close lifetime measured independently. Complete native packet association/cancel/
completion scheduling remains part of the full process goal; no successful host
packet forwarding or invented completion state is permitted. Exact native source,
rows, metadata and failures precede production implementation. All OS owning
surfaces, full/CAPI/build/integration gates and paired red publication are required.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
|---|---|---|---|---|---|---|
| P1 | Constructor has PHANDLE,ACCESS_MASK,optional OA | Pinned PHNT ntioapi declaration and ordinary request[0x1103318,1,0] | Export/transport | x86RET12,x64/ARM caller returns | Installed leaf/caller cleanup differs | confirmed installed ARM64/WoW64 leaves and caller cleanup |
| P2 | New packet is initially signaled when SYNCHRONIZE granted |317-row ARM64/x64 captures report wait0 and WaitCompletionPacket type | Private state/wait model | repeated wait, duplicate alias, association transitions | A second wait times out or different token/profile changes state | confirmed four profiles; repeated waits and final-close alias lifetime |
| P3 | Constructor access/attribute mechanics may differ from completion port | Native generic-all0xF0001 vs explicit0x1F0001; generic R/W/X0x20001 | Access mapping and permission checks | MAXIMUM_ALLOWED,combined bits,reduced token,early faults | Native effective grant differs | confirmed four measured profiles; different SID/build remains unknown |

Change surface: NT export, dedicated packet code/private object/wait consumers,
constructor/replay/native-leaf/lifetime tests and evidence/runtime index; C ABI and
root schemas/permissions remain unchanged. Main UI/mesh/crypto/persistence/release
have no changed contract. Shared archives/plugin/CLI/SDK and all OSes consume the
private HLE change. The public wait whitelist now admits legacy/IR timers and packets; all four
comparative native profiles confirm dormant waits and event-alias consumption.
Medium: retained full-suite failures,physical x64 Windows/alternate caller/build,
package/live IDA/Qt proof. Full application completion remains unproven.
