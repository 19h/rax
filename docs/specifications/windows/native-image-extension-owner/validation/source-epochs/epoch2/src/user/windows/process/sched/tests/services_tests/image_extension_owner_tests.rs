//! Present-owner counterpart to the independently retained absent-image corpus.
use super::*;
use crate::user::windows::object_namespace::{ImageSection, image_extension_tests};
#[path = "image_extension_owner_replay_tests.rs"]
mod replay;

#[cfg(windows)]
#[path = "image_extension_owner_leaf_tests.rs"]
mod installed;

#[test]
fn image_extension_owner_queries_full_views_tail_addresses_aliases_and_release() {
    for arch in WinArch::ALL {
        let (mut process, mut t, scratch) = setup(arch);
        let p = process.state_mut();
        let image = image_extension_tests::template(0x7000_0000);
        let section = ImageSection::with_test_image(image.clone());
        let base = 0x5000_0000;
        p.vm.map_section_image(section, &image, base, base, 16384, 0, 8192, false, false)
            .unwrap();
        for offset in [0, 1, 8191, 8192, 12288, 16383] {
            let sp = t.cpu.sp();
            assert_eq!(
                query(p, &mut t, base + offset, 6, scratch + 512, 24, 0),
                STATUS_SUCCESS
            );
            t.cpu.set_sp(sp);
            assert_eq!(
                p.space.u32(scratch + 512 + 2 * arch.ptr_size()).unwrap(),
                0x40
            );
            assert_eq!(
                p.space
                    .ptr(scratch + 512 + arch.ptr_size(), arch.ptr_size())
                    .unwrap(),
                8192
            );
            for kind in [0, 1] {
                for alias in [0, 8, 16, 256] {
                    let out = scratch + 128;
                    let returned = out + alias;
                    p.space.wr(out, &[0xA5; 512]).unwrap();
                    p.space.w32(out, kind).unwrap();
                    p.space.w32(out + 4, 0).unwrap();
                    let sp = t.cpu.sp();
                    let status = query(p, &mut t, base + offset, 14, out, 24, returned);
                    t.cpu.set_sp(sp);
                    if arch == WinArch::X86 {
                        assert_eq!(status, STATUS_INVALID_INFO_CLASS);
                        continue;
                    }
                    assert_eq!(status, STATUS_SUCCESS);
                    let mut expected = [0u8; 24];
                    expected[..4].copy_from_slice(&kind.to_le_bytes());
                    expected[8..16]
                        .copy_from_slice(&(8192u64 + 4096 * u64::from(kind)).to_le_bytes());
                    expected[16..24].copy_from_slice(&4096u64.to_le_bytes());
                    if alias < 24 {
                        expected[alias as usize..alias as usize + 8]
                            .copy_from_slice(&24u64.to_le_bytes());
                    }
                    assert_eq!(p.space.bytes(out, 24).unwrap(), expected);
                    assert_eq!(p.space.u64(returned).unwrap(), 24);
                }
            }
        }
        p.vm.release(base).unwrap();
        let sp = t.cpu.sp();
        p.space.w64(scratch, 0).unwrap();
        let status = query(p, &mut t, base + 8192, 14, scratch, 24, 0);
        t.cpu.set_sp(sp);
        assert_eq!(
            status,
            if arch == WinArch::X86 {
                STATUS_INVALID_INFO_CLASS
            } else {
                STATUS_MEMORY_NOT_ALLOCATED
            }
        );
    }
}
