use super::*;
#[test]
fn native_system_wnf_installed_leaves_replay_792_cases_per_producer() {
    let host = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    let mut profiles = vec![(host, false), (WinArch::X86, true)];
    if host != WinArch::X86 {
        profiles.insert(1, (WinArch::X86, false));
    }
    for (arch, laa) in profiles {
        let (mut process, mut thread, b) = setup(arch, laa, true);
        let p = process.state_mut();
        assert_eq!(fresh(p, &mut thread, b, true, true), (216, 0));
        assert_eq!(transitions(p, &mut thread, b, true, false), 288);
        assert_eq!(transitions(p, &mut thread, b, true, true), 288);
    }
}
