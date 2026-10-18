# Proposed bounded follow-on contract (pending complete native evidence)

Preserve the published SessionId/AppContainer semantics and independent process-session PEB value. Extend only native scalar query classes whose complete native matrices establish a compatible buffer/access contract. Keep absent capture explicit. Primary-token properties cannot be substituted for impersonation-token properties.

Candidate observations may disagree with Win32 documentation or PHNT comments. In particular, the first native NtQueryInformationToken survey could not complete its four-byte reference for classes21,45,46. The failing condition conflates NTSTATUS and returned length. Version2 records both and does not require success before measuring the caller matrix. No width or status has been inferred from these failures.

Acquisition must correspond to native NtQueryInformationToken behavior rather than assume every GetTokenInformation selector has an identical kernel implementation. Use private process-token handles only during runtime selection; no guest handler performs a host call. Include the entire selected property map in the repeated immutable snapshot comparison. On older kernels, unavailable properties must remain individually unavailable without disabling unrelated token behavior.

A compact canonical corpus may share byte-identical measured row sets between selectors, but the checked raw logs must bind every selector/profile independently. Values are captured, not invented constants. Nonzero property tests, missing capture/subject, handle rights/types, class29 impersonation behavior, guard ordering, PEB independence, x86 and x86-LAA ABI paths remain required. Any property with a different native failure/probe contract needs its own complete branch or must remain outside the admitted selector set.

No Rust implementation or repository changes exist for this group yet. The preceding SessionId group is still running its final Windows regression suite.

The second survey also includes TokenElevationType18 and TokenMandatoryPolicy27, whose documented structures each contain one fixed-width value. Their actual native status, width and caller matrix must still be measured before admission. No enum/flag range is assumed from their names.

## Initial version2 native64 observations

- Class21 succeeds with ReturnLength1 and writes only the first byte of the initialized four-byte reference buffer (A5A5A501). Native caller alignment failures still occur. Size and alignment must be separate concepts.
- Classes45 and46 return C0000003, but only after the common caller probes: their full matrices also include access violations, guard violations and datatype-misalignment results. Invalid handles do not override the invalid-class result. Returning immediately before probes would be incorrect.
- Class18 and27 are successful four-byte values, both1 in this controller context.
- The x86 class40 difference is not understood yet; do not assume it is unsupported. Version1 only established that status!=0 OR ReturnLength!=4.

A prospective metadata record needs to distinguish an absent capture, a captured scalar value, and a captured native invalid-class capability. The guest dispatch phase for each must be supported by its native matrix. A native scalar capture should call NtQueryInformationToken directly on its private handle, avoiding assumptions that Win32 GetTokenInformation implements every selector with an identical kernel query.

The class21 width observation belongs to native NtQueryInformationToken. Microsoft's TOKEN_INFORMATION_CLASS text describes the Win32 GetTokenInformation surface; that separate wrapper has not been measured for class21 here. Do not label its documentation incorrect or assume it uses the native byte layout. The native producer owns this emulator boundary.

## Version2 x86 observations

Class21 also returns one byte under x86. Classes40 and45 return C0000003 for all1830 rows before any caller probes, including invalid/guarded buffers. Class46 instead follows the native64 common-probe failure ordering before C0000003. Therefore a generic unavailable-selector check at one shared point would be incorrect: WoW64 classes40/45 need a pre-probe path, while native64 class45 and all class46 profiles have a post-probe/pre-handle path. x86-LAA results are still pending.

Class20 TOKEN_ELEVATION is the sixth behavior group: after common probes it writes ReturnLength4 before handle validation, and requires length exactly4 (C0000004 for smaller or larger lengths). Other successful DWORD selectors require a minimum of4 and publish length after validating the token. The draft query code now distinguishes this measured phase.
