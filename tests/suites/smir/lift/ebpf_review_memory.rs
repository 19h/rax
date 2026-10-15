use super::*;
use rax::isa::ebpf::{
    EbpfMemWidth, EbpfMemory, MM_BYTECODE_START, MM_STACK_START, SbpfMemoryMapping,
};

#[test]
fn bytecode_fetch_and_guest_data_permissions_follow_the_versioned_layout() {
    let text = encode_insn(0x95, 0, 0, 0, 0, Endian::Little);
    for version in [
        SbpfVersion::V0,
        SbpfVersion::V1,
        SbpfVersion::V2,
        SbpfVersion::V3,
        SbpfVersion::V4,
    ] {
        let lower = matches!(version, SbpfVersion::V3 | SbpfVersion::V4);
        let config = EbpfConfig::solana(version);
        let mut map = SbpfMemoryMapping::new_solana_with_config(
            &config,
            &text,
            if lower { 0 } else { MM_BYTECODE_START },
            &text,
            4096,
            0,
            &[],
        )
        .unwrap();
        assert_eq!(
            EbpfMemory::load(
                &mut map,
                MM_BYTECODE_START,
                EbpfMemWidth::B1,
                false,
                Endian::Little
            )
            .is_err(),
            lower
        );
        assert_eq!(
            SmirMemory::probe(&map, MM_BYTECODE_START, 1, false).is_err(),
            lower
        );
        assert_eq!(
            rax::smir::lift::MemoryReader::read(&map, MM_BYTECODE_START, 8).unwrap(),
            text
        );
        let mut cpu = EbpfCpu::new(config);
        cpu.regs.pc = MM_BYTECODE_START;
        assert_eq!(cpu.run(&mut map), EbpfExit::Exit { r0: 0 });
    }
}

#[test]
fn v0_gap_is_inaccessible() {
    let mut map = SbpfMemoryMapping::new_solana_with_config(
        &EbpfConfig::solana(SbpfVersion::V0),
        &[],
        MM_BYTECODE_START,
        &[],
        8192,
        0,
        &[],
    )
    .unwrap();
    assert!(EbpfMemory::read_bytes(&mut map, MM_STACK_START + 4096, &mut [0]).is_err());
}

#[test]
fn v0_access_starting_in_a_frame_uses_contiguous_backing_across_its_gap() {
    let mut map = SbpfMemoryMapping::new_solana_with_config(
        &EbpfConfig::solana(SbpfVersion::V0),
        &[],
        MM_BYTECODE_START,
        &[],
        8192,
        0,
        &[],
    )
    .unwrap();
    EbpfMemory::write_bytes(&mut map, MM_STACK_START + 4094, &[0xaa, 0xbb]).unwrap();
    EbpfMemory::write_bytes(&mut map, MM_STACK_START + 8192, &[0xcc, 0xdd]).unwrap();
    assert_eq!(
        EbpfMemory::load(
            &mut map,
            MM_STACK_START + 4094,
            EbpfMemWidth::B4,
            false,
            Endian::Little
        )
        .unwrap(),
        0xddcc_bbaa
    );
    assert!(SmirMemory::probe(&map, MM_STACK_START + 4094, 4, true).is_ok());
    EbpfMemory::write_bytes(&mut map, MM_STACK_START + 4094, &[1, 2, 3, 4]).unwrap();
    assert_eq!(
        EbpfMemory::load(
            &mut map,
            MM_STACK_START + 8192,
            EbpfMemWidth::B2,
            false,
            Endian::Little
        )
        .unwrap(),
        0x0403
    );
    assert!(
        EbpfMemory::load(
            &mut map,
            MM_STACK_START + 8192 + 4094,
            EbpfMemWidth::B4,
            false,
            Endian::Little
        )
        .is_err()
    );
}

#[test]
fn versioned_rodata_layout_and_stack_gaps_apply_to_all_access_paths() {
    let text = encode_insn(0x95, 0, 0, 0, 0, Endian::Little);
    for version in [
        SbpfVersion::V0,
        SbpfVersion::V1,
        SbpfVersion::V2,
        SbpfVersion::V3,
        SbpfVersion::V4,
    ] {
        for gaps in [false, true] {
            let config = EbpfConfig {
                enable_stack_frame_gaps: gaps,
                ..EbpfConfig::solana(version)
            };
            let rodata_addr = if matches!(version, SbpfVersion::V3 | SbpfVersion::V4) {
                0x100
            } else {
                MM_BYTECODE_START + 0x100
            };
            let mut map = SbpfMemoryMapping::new_solana_with_config(
                &config,
                &text,
                rodata_addr,
                b"readonly",
                8192,
                64,
                b"input",
            )
            .unwrap();
            assert_eq!(
                EbpfMemory::load(
                    &mut map,
                    rodata_addr,
                    EbpfMemWidth::B8,
                    false,
                    Endian::Little
                )
                .unwrap(),
                u64::from_le_bytes(*b"readonly")
            );
            assert!(
                EbpfMemory::store(&mut map, rodata_addr, 0, EbpfMemWidth::B8, Endian::Little)
                    .is_err()
            );
            let gapped = version == SbpfVersion::V0 && gaps;
            let second_frame = MM_STACK_START + if gapped { 8192 } else { 4096 };
            EbpfMemory::store(&mut map, second_frame, 42, EbpfMemWidth::B8, Endian::Little)
                .unwrap();
            assert_eq!(
                EbpfMemory::load(
                    &mut map,
                    second_frame,
                    EbpfMemWidth::B8,
                    false,
                    Endian::Little
                )
                .unwrap(),
                42
            );
            for (offset, len) in [(4096, 1), (8191, 1), (4095, 2), (4092, 8)] {
                let addr = MM_STACK_START + offset;
                let starts_in_gap = gapped && (offset / 4096) % 2 == 1;
                let mut bytes = vec![0; len];
                assert_eq!(
                    EbpfMemory::read_bytes(&mut map, addr, &mut bytes).is_err(),
                    starts_in_gap
                );
                assert_eq!(
                    SmirMemory::read(&mut map, addr, &mut bytes).is_err(),
                    starts_in_gap
                );
                assert_eq!(
                    SmirMemory::probe(&map, addr, len, false).is_err(),
                    starts_in_gap
                );
                // Explicit executable mappings never lift writable stack bytes.
                assert!(rax::smir::lift::MemoryReader::read(&map, addr, len).is_err());
                assert_eq!(
                    EbpfMemory::write_bytes(&mut map, addr, &vec![1; len]).is_err(),
                    starts_in_gap
                );
            }
            assert!(
                EbpfMemory::read_bytes(
                    &mut map,
                    MM_STACK_START + if gapped { 12288 } else { 8192 },
                    &mut [0]
                )
                .is_err()
            );
            assert!(EbpfMemory::read_bytes(&mut map, u64::MAX, &mut [0; 2]).is_err());
        }
    }
}

#[test]
fn executable_rodata_can_contain_text_without_shadowing_it() {
    let text = encode_insn(0x95, 0, 0, 0, 0, Endian::Little);
    let mut image = text.to_vec();
    image.extend_from_slice(b"readonly");
    let config = EbpfConfig::solana(SbpfVersion::V0);
    let mut map = SbpfMemoryMapping::new_solana_with_config(
        &config,
        &text,
        MM_BYTECODE_START,
        &image,
        4096,
        64,
        b"",
    )
    .unwrap();
    let mut cpu = EbpfCpu::new(config.clone());
    cpu.regs.pc = MM_BYTECODE_START;
    assert_eq!(cpu.run(&mut map), EbpfExit::Exit { r0: 0 });
    assert_eq!(
        EbpfMemory::load(
            &mut map,
            MM_BYTECODE_START + 8,
            EbpfMemWidth::B8,
            false,
            Endian::Little
        )
        .unwrap(),
        u64::from_le_bytes(*b"readonly")
    );
    assert!(
        SbpfMemoryMapping::new_solana_with_config(&config, &text, 0, &image, 4096, 64, b"")
            .is_err()
    );
    assert!(
        SbpfMemoryMapping::new_solana_with_config(
            &config,
            &text,
            MM_BYTECODE_START,
            b"mismatch",
            4096,
            64,
            b""
        )
        .is_err()
    );
}
