//! Process-session identity remains separate from a token's session DWORD.
use super::*;
use crate::user::windows::process::WindowsProcess;

#[test]
fn native_token_session_id_keeps_peb_process_session_separate_from_token_value() {
    for arch in WinArch::ALL {
        let image: &[u8] = match arch {
            WinArch::Arm64 => {
                include_bytes!("../../../../../tests/fixtures/user/windows/bin/arm64/smoke.exe")
            }
            WinArch::X64 => {
                include_bytes!("../../../../../tests/fixtures/user/windows/bin/x64/smoke.exe")
            }
            WinArch::X86 => {
                include_bytes!("../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
            }
        };
        let cfg = WindowsConfig::embedded("C:\\session.exe", vec![], vec![], 4096).unwrap();
        let mut process = WindowsProcess::spawn_image(cfg, image.to_vec()).unwrap();
        let p = process.state_mut();
        let address = p.peb + offsets(arch).peb_session_id;
        assert_eq!(p.space.u32(address).unwrap(), 0);
        for (process_session, token_session) in
            [(0, 7), (7, 0), (0x8000_0000, 0x1234_5678), (u32::MAX, 1)]
        {
            let queries =
                crate::user::windows::wnf::Queries::selected(vec![], process_session).unwrap();
            p.objects.select_wnf_catalog(
                crate::user::windows::wnf::tests::empty_subject_catalog(false)
                    .with_primary_token_session_id(token_session)
                    .with_queries(queries),
            );
            p.space.wr(address - 4, &[0xA5; 12]).unwrap();
            initialize_session_id(p).unwrap();
            assert_eq!(p.space.u32(address).unwrap(), process_session);
            assert_eq!(p.objects.primary_token_session_id(), Some(token_session));
            assert_eq!(p.space.u32(address - 4).unwrap(), 0xA5A5_A5A5);
            assert_eq!(p.space.u32(address + 4).unwrap(), 0xA5A5_A5A5);
        }
    }
}
