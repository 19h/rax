use crate::user::image::pe::{IMAGE_SCN_MEM_SHARED, IMAGE_SCN_MEM_WRITE, PeImage};
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::arch::WinArch;
use crate::user::windows::memory::{Mem, prot};
use crate::user::windows::process::{WindowsConfig, WindowsProcess};

fn shared_image(arch: WinArch) -> (Vec<u8>, u64) {
    let mut bytes = match arch {
        WinArch::X86 => {
            include_bytes!("../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe").to_vec()
        }
        WinArch::X64 => {
            include_bytes!("../../../../../tests/fixtures/user/windows/bin/x64/smoke.exe").to_vec()
        }
        WinArch::Arm64 => {
            include_bytes!("../../../../../tests/fixtures/user/windows/bin/arm64/smoke.exe")
                .to_vec()
        }
    };
    let pe = PeImage::parse(bytes.clone()).unwrap();
    let index = pe
        .sections()
        .iter()
        .position(|section| section.name == *b".rdata\0\0")
        .unwrap();
    let nt = pe.headers().nt_offset as usize;
    let optional_size = u16::from_le_bytes(bytes[nt + 20..nt + 22].try_into().unwrap()) as usize;
    let offset = nt + 24 + optional_size + index * 40 + 36;
    let section = &pe.sections()[index];
    let flags = section.characteristics | IMAGE_SCN_MEM_SHARED | IMAGE_SCN_MEM_WRITE;
    bytes[offset..offset + 4].copy_from_slice(&flags.to_le_bytes());
    (bytes, u64::from(section.virtual_address))
}

#[test]
fn region_copy_charge_respects_initial_shared_image_and_explicit_protection_all_abis() {
    for arch in WinArch::ALL {
        let (bytes, rva) = shared_image(arch);
        let mut config =
            WindowsConfig::embedded("C:\\shared.exe", Vec::new(), Vec::new(), 4096).unwrap();
        config.arena_bytes = 64 << 20;
        let mut process = WindowsProcess::spawn_image(config, bytes).unwrap();
        let p = process.state_mut();
        let base = p.modules.exe().base;
        let page = base + rva;
        let before = p.vm.allocation_region(base).unwrap().commit;
        assert_eq!(p.vm.query(page).unwrap().protect, prot::READWRITE);
        let value = p.space.u8(page).unwrap();
        p.space.w8(page, value).unwrap();
        assert_eq!(p.vm.allocation_region(base).unwrap().commit, before);
        p.vm.protect(page, PAGE_SIZE, prot::READWRITE).unwrap();
        assert_eq!(
            p.vm.allocation_region(base).unwrap().commit,
            before + PAGE_SIZE
        );
        p.vm.protect(page, PAGE_SIZE, prot::READONLY).unwrap();
        assert_eq!(p.vm.allocation_region(base).unwrap().commit, before);
        p.vm.protect(page, PAGE_SIZE, prot::READWRITE).unwrap();
        p.space.w8(page, value).unwrap();
        p.vm.protect(page, PAGE_SIZE, prot::READONLY).unwrap();
        assert_eq!(
            p.vm.allocation_region(base).unwrap().commit,
            before + PAGE_SIZE
        );
    }
}
