//! Helper-backed XMM MASKMOVDQU/VMASKMOVDQU lowering tests.

use super::*;
use crate::smir::lower::X86_GUEST_ZMM_OFFSET;

fn lower_lifted_maskmovdqu(
    bytes: &[u8],
    level: crate::smir::optimize::OptLevel,
    preserve_vectors: bool,
) -> Vec<u8> {
    let mut code = bytes.to_vec();
    code.extend_from_slice(&[0xEB, 0x00]);
    let reader = TestReader {
        base: 0x1000,
        bytes: code,
    };
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let mut block = lifter
        .lift_block(0x1000, &reader, &mut context)
        .unwrap_or_else(|error| panic!("lift MASKMOVDQU form {bytes:02X?}: {error:?}"));
    block.set_terminator(Terminator::Return { values: vec![] });
    let block_id = block.id;
    let mut function = SmirFunction::new(FunctionId(0), block_id, 0x1000);
    function.add_block(block);
    crate::smir::optimize::optimize_function(&mut function, level);
    let excluded = std::collections::HashMap::new();
    assert!(
        crate::smir::lower::runtime::is_native_clobber_safe_excluding(&function, &excluded, true),
        "helper gate rejected {bytes:02X?} after {level:?}: {:?}",
        function.blocks[0].ops
    );
    assert!(
        crate::smir::lower::runtime::uses_x86_maskmovdqu_state_excluding(&function, &excluded),
        "XMM state requirement missing for {bytes:02X?} after {level:?}"
    );
    assert!(!crate::smir::lower::runtime::uses_x86_native_vectors_excluding(&function, &excluded));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(preserve_vectors);
    let result = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("lower {bytes:02X?} after {level:?}: {error:?}"));
    assert!(result.relocations.is_empty());
    lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("finalize {bytes:02X?} after {level:?}: {error:?}"))
}

fn state_load_rsi(offset: i32) -> Vec<u8> {
    let mut bytes = vec![0x48, 0x8B, 0xB0];
    bytes.extend_from_slice(&offset.to_le_bytes());
    bytes
}

#[test]
fn maskmovdqu_emits_state_backed_ordered_byte_helpers_at_all_opt_levels() {
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O1,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for bytes in [&[0x66, 0x0F, 0xF7, 0xC1][..], &[0xC5, 0xF9, 0xF7, 0xC1][..]] {
            let code = lower_lifted_maskmovdqu(bytes, level, false);
            for offset in [
                X86_GUEST_ZMM_OFFSET,
                X86_GUEST_ZMM_OFFSET + 8,
                X86_GUEST_ZMM_OFFSET + 64,
                X86_GUEST_ZMM_OFFSET + 72,
            ] {
                let load = state_load_rsi(offset);
                assert!(
                    code.windows(load.len()).any(|window| window == load),
                    "missing state-backed XMM snapshot {offset:#x} for {bytes:02X?} after {level:?}: {code:02X?}"
                );
            }
            for store in [
                &[0x48, 0x89, 0x74, 0x24, 0x10][..],
                &[0x48, 0x89, 0x74, 0x24, 0x18][..],
                &[0x48, 0x89, 0x74, 0x24, 0x20][..],
                &[0x48, 0x89, 0x74, 0x24, 0x28][..],
            ] {
                assert!(
                    code.windows(store.len()).any(|window| window == store),
                    "missing stack snapshot {store:02X?} for {bytes:02X?} after {level:?}: {code:02X?}"
                );
            }
            for lane in 0..16u8 {
                let test = [0xF6, 0x44, 0x24, 0x18 + lane, 0x80];
                assert!(
                    code.windows(test.len()).any(|window| window == test),
                    "lane {lane} mask test missing for {bytes:02X?} after {level:?}: {code:02X?}"
                );
            }
            assert_eq!(
                code.windows(5)
                    .filter(|window| *window == [0xB9, 0x01, 0x00, 0x00, 0x00])
                    .count(),
                16,
                "one exact byte helper is required per lane for {bytes:02X?} after {level:?}"
            );
            assert_eq!(
                code.windows(5)
                    .filter(|window| *window == [0x48, 0x8D, 0x64, 0x24, 0x20])
                    .count(),
                17,
                "sixteen fault paths plus success must release the 32-byte snapshot for {bytes:02X?} after {level:?}"
            );
            assert!(
                !code
                    .windows(5)
                    .any(|window| window == [0xF3, 0x0F, 0x7F, 0x04, 0x24]),
                "state-only regions must not read inactive host XMM state"
            );
        }
    }
}

#[test]
fn maskmovdqu_uses_live_host_xmm_snapshot_in_mixed_vector_regions() {
    let code = lower_lifted_maskmovdqu(
        &[0x66, 0x0F, 0xF7, 0xC1],
        crate::smir::optimize::OptLevel::O2,
        true,
    );
    assert!(
        code.windows(5)
            .any(|window| window == [0xF3, 0x0F, 0x7F, 0x04, 0x24]),
        "data XMM0 must be snapshotted from live native state: {code:02X?}"
    );
    assert!(
        code.windows(6)
            .any(|window| window == [0xF3, 0x0F, 0x7F, 0x4C, 0x24, 0x10]),
        "mask XMM1 must be snapshotted from live native state: {code:02X?}"
    );
}

#[test]
fn maskmovdqu_addr32_emits_lane_wrapping_before_optional_fs_base() {
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O1,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let addr32 = lower_lifted_maskmovdqu(&[0x67, 0x66, 0x0F, 0xF7, 0xC1], level, false);
        assert!(
            addr32
                .windows(2)
                .filter(|window| *window == [0x89, 0xF6])
                .count()
                >= 16,
            "each addr32 lane must zero-extend EDI in ESI after {level:?}: {addr32:02X?}"
        );
        assert!(addr32.windows(3).any(|window| window == [0x83, 0xC6, 0x0F]));

        let fs = lower_lifted_maskmovdqu(&[0x64, 0x67, 0x66, 0x0F, 0xF7, 0xC1], level, false);
        assert!(
            fs.windows(2)
                .filter(|window| *window == [0x89, 0xFF])
                .count()
                >= 16,
            "each FS addr32 lane must zero-extend EDI after {level:?}: {fs:02X?}"
        );
        assert!(fs.windows(3).any(|window| window == [0x83, 0xC7, 0x0F]));
        assert!(
            fs.windows(3)
                .filter(|window| *window == [0x48, 0x01, 0xFE])
                .count()
                >= 16,
            "each wrapped lane must add FS base after {level:?}: {fs:02X?}"
        );
    }
}
