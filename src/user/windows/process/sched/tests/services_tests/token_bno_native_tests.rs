//! Independent native query bytes versus the installed NTDLL guest entry.
use super::*;
use std::ffi::c_void;

type Handle = *mut c_void;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtOpenProcessToken(process: Handle, access: u32, output: *mut Handle) -> i32;
    fn NtQueryInformationToken(
        token: Handle,
        class: u32,
        output: *mut c_void,
        bytes: u32,
        returned: *mut u32,
    ) -> i32;
}
struct Owned(Handle);
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: this independent native token is exclusively owned here.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[test]
fn native_token_bno_installed_leaves_match_independent_bytes_and_alignment() {
    let mut raw = std::ptr::null_mut();
    // SAFETY: current process, TOKEN_QUERY and exclusive HANDLE output.
    assert_eq!(
        unsafe { NtOpenProcessToken(GetCurrentProcess(), 8, &mut raw) },
        0
    );
    assert!(!raw.is_null());
    let token = Owned(raw);
    let host = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    let mut profiles = vec![(host, false)];
    if host != WinArch::X86 {
        profiles.extend([(WinArch::X86, false), (WinArch::X86, true)]);
    }
    let native_width = std::mem::size_of::<usize>();
    let mut reference = [0u64; 128];
    let mut reference_bytes = 0u32;
    // SAFETY: live private token and 1024 initialized writable bytes.
    assert_eq!(
        unsafe {
            NtQueryInformationToken(
                token.0,
                44,
                reference.as_mut_ptr().cast(),
                1024,
                &mut reference_bytes,
            )
        },
        0
    );
    assert!(reference_bytes >= 2 * native_width as u32 && reference_bytes <= 1024);
    for (arch, laa) in profiles {
        let (mut process, mut thread, base, handles) = setup_runtime(arch, laa, true);
        let p = process.state_mut();
        let module = crate::user::windows::loader::load_dll(p, "ntdll.dll").unwrap();
        let header_delta = 2 * native_width as u32 - 2 * arch.ptr_size() as u32;
        let guest_required = reference_bytes - header_delta;
        for length in [0, 1, 4, 7, 8, 15, 16, 17, 31, 32, 64, 128, 288, 1024] {
            // Translate only capacity, preserving the zero-length/no-probe case.
            // Both native pointer-width layouts are independently recorded in
            // the four-profile replay corpus.
            let native_length = if length < guest_required {
                length
            } else {
                length + header_delta
            };
            // Native64's alignment failures cannot serve as a WoW64 oracle:
            // class44's converter permits unaligned stores. Those controls
            // are covered by all raw WoW64 replay rows. The live comparison
            // shares only aligned destinations across pointer widths.
            let alignments: &[usize] = if arch == host {
                &[0, 1, 2, 3, 4, 5, 6, 7]
            } else {
                &[0, 4]
            };
            for &alignment in alignments {
                let mut native_data = [0xa5a5_a5a5_a5a5_a5a5u64; 260];
                let mut native_return = 0xa5a5_a5a5u32;
                let native_output = (native_data.as_mut_ptr() as *mut u8).wrapping_add(alignment);
                // SAFETY: independently owned query token and initialized aligned
                // 2080-byte storage; each requested range is inside that allocation.
                let status = unsafe {
                    NtQueryInformationToken(
                        token.0,
                        44,
                        native_output.cast(),
                        native_length,
                        &mut native_return,
                    )
                } as u32;
                let mut expected = [0u8; 1024];
                // SAFETY: all 1024 bytes following the selected offset are owned.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        native_output,
                        expected.as_mut_ptr(),
                        expected.len(),
                    );
                }
                let (output, returned) = destinations(p, base, 0);
                let output = output + alignment as u64;
                let actual = super::super::super::virtual_memory_tests::installed::invoke(
                    p,
                    &mut thread,
                    module,
                    "NtQueryInformationToken",
                    &[handles[1], 44, output, u64::from(length), returned],
                );
                assert_eq!(actual, status, "{arch}/{length}/{alignment}");
                let expected_return = if native_return == 0xa5a5_a5a5 {
                    native_return
                } else {
                    native_return - header_delta
                };
                assert_eq!(p.space.u32(returned).unwrap(), expected_return);
                if status == 0 {
                    let mut raw_pointer = [0u8; 8];
                    raw_pointer[..native_width].copy_from_slice(&expected[..native_width]);
                    let pointer = u64::from_le_bytes(raw_pointer);
                    let enabled = expected[native_width];
                    assert!(enabled <= 1);
                    let prefix = if enabled == 0 {
                        assert_eq!(pointer, 0);
                        None
                    } else {
                        assert_eq!(pointer, native_output as u64 + 2 * native_width as u64);
                        let tail = &expected[2 * native_width..native_return as usize];
                        assert_eq!(tail[tail.len() - 2..], [0, 0]);
                        Some(tail[..tail.len() - 2].to_vec())
                    };
                    // Build the independent guest-width oracle from native fields.
                    expected.fill(0xa5);
                    let guest_width = arch.ptr_size() as usize;
                    let guest_pointer = if enabled != 0 {
                        output + 2 * arch.ptr_size()
                    } else {
                        0
                    };
                    expected[..guest_width]
                        .copy_from_slice(&guest_pointer.to_le_bytes()[..guest_width]);
                    expected[guest_width] = enabled;
                    if let Some(mut prefix) = prefix {
                        prefix.extend_from_slice(&[0, 0]);
                        expected[2 * guest_width..2 * guest_width + prefix.len()]
                            .copy_from_slice(&prefix);
                    }
                    // A native64 oracle and WoW64 guest have different header sizes.
                    assert_eq!(p.space.u32(returned).unwrap(), guest_required);
                }
                let mut actual_bytes = [0u8; 1024];
                p.space.rd(output, &mut actual_bytes).unwrap();
                assert_eq!(actual_bytes, expected, "output {arch}/{length}/{alignment}");
            }
        }
    }
}
