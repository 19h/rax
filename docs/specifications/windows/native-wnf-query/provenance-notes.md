# Source and evidence corrections

The V1/V2 WNF query producers attempted Temporary lifetime 3, Process scope 3.
The measured kernel rejected that creation with STATUS_INVALID_PARAMETER.
Those failure records remain separate from all successful matrices.

V3, the boundary producer and V4/V5 use Temporary lifetime 3, System scope 0.
Some unchanged controller descriptions and failure-operation strings still say
"process". The compiled producer arguments, decoded returned names and raw
cleanup records establish System scope; the descriptive labels are stale.
Do not count the successful matrices as private Process-scope creation proof.
Temporary ownership remains the creating probe process with explicit delete.

V4 labels a fourth name "owned-denied", but its constructor still gives read
permission. V4 establishes no read denial. V5 source changes the actual fourth
state descriptor to mask 0x10002 (DELETE|WRITE, no READ), and its denied results
are confirmed on the independently inspected ARM64, x86 and x86-LAA producers.
The x64 V5 producer completed its calls and cleanup, but Defender quarantine
prevented executable identity inspection. Its complete stdout is corroborative
and is not counted as independently verified native producer evidence.

The registry inventory parses complete self-relative descriptor extents before
reading the trailer. A four-byte trailer is the maximum state-data length; a
20-byte trailer adds a 16-byte TypeId. The actual NTDLL continuation's registry
entry has maximum length 8 and the same TypeId bytes captured in the call.
This inventory is offline inspection of the retained registry snapshot, not
additional queries of unrelated host state.
