//! Helper-backed MMX m64-source lowering tests.

use super::*;

fn lift_mmx_m64_function(bytes: &[u8]) -> SmirFunction {
    let mut code = bytes.to_vec();
    code.extend_from_slice(&[0xEB, 0x00]); // terminate the block without extra semantic ops
    let reader = TestReader {
        base: 0x1000,
        bytes: code,
    };
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let mut block = lifter
        .lift_block(0x1000, &reader, &mut context)
        .unwrap_or_else(|error| panic!("lift MMX m64 instruction {bytes:02X?}: {error:?}"));
    block.set_terminator(Terminator::Return { values: vec![] });
    let block_id = block.id;
    let mut function = SmirFunction::new(FunctionId(0), block_id, 0x1000);
    function.add_block(block);
    function
}

fn lower_mmx_m64_instruction(bytes: &[u8]) -> (SmirFunction, Vec<u8>) {
    let function = lift_mmx_m64_function(bytes);
    let excluded = HashMap::new();
    assert!(
        crate::smir::lower::runtime::is_native_clobber_safe_excluding(&function, &excluded, true),
        "helper gate rejected MMX m64 instruction {bytes:02X?}: {:?}",
        function.blocks[0].ops
    );
    assert!(
        crate::smir::lower::runtime::x86_native_mmx_pairs_valid_excluding(&function, &excluded),
        "state-pair gate rejected MMX m64 instruction {bytes:02X?}: {:?}",
        function.blocks[0].ops
    );
    assert!(
        !crate::smir::lower::runtime::uses_x86_native_vectors_excluding(&function, &excluded),
        "MMX m64 instruction must not select the AVX-512 trampoline: {bytes:02X?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_mmx_helpers(true);
    let result = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("lower MMX m64 instruction {bytes:02X?}: {error:?}"));
    assert!(result.relocations.is_empty());
    let code = lowerer.finalize().expect("finalize MMX m64 instruction");
    (function, code)
}

fn assert_contains(bytes: &[u8], expected: &[u8], name: &str) {
    assert!(
        bytes
            .windows(expected.len())
            .any(|window| window == expected),
        "missing {name} {expected:02X?} in {bytes:02X?}"
    );
}

#[test]
fn mmx_m64_source_lifters_gate_and_lower_every_native_opcode_family() {
    // Every classic two-operand MMX operation currently admitted for native
    // register sources and architecturally encoded with an m64 source. Low
    // PUNPCKL* forms are intentionally absent: their MMX memory source is m32.
    let map_0f = [
        0xD4, 0xD8, 0xD9, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE8,
        0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0xEF, 0xF1, 0xF2, 0xF3, 0xF5, 0xF6, 0xF8, 0xF9, 0xFA,
        0xFB, 0xFC, 0xFD, 0xFE, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x74, 0x75,
        0x76, 0xD1, 0xD2, 0xD3, 0xD5, 0xDA,
    ];
    for opcode in map_0f {
        let guest = [0x0F, opcode, 0x1B]; // operation mm3, [rbx]
        let (_, code) = lower_mmx_m64_instruction(&guest);
        assert_contains(
            &code,
            &[0x0F, opcode, 0x1C, 0x24],
            &format!("0F {opcode:02X} mm3,[rsp]"),
        );
    }

    for opcode in [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x1C, 0x1D, 0x1E,
    ] {
        let guest = [0x0F, 0x38, opcode, 0x1B];
        let (_, code) = lower_mmx_m64_instruction(&guest);
        assert_contains(
            &code,
            &[0x0F, 0x38, opcode, 0x1C, 0x24],
            &format!("0F 38 {opcode:02X} mm3,[rsp]"),
        );
    }

    let (_, shuffle) = lower_mmx_m64_instruction(&[0x0F, 0x70, 0x1B, 0x1B]);
    assert_contains(
        &shuffle,
        &[0x0F, 0x70, 0x1C, 0x24, 0x1B],
        "PSHUFW mm3,[rsp],0x1B",
    );

    let (_, align) = lower_mmx_m64_instruction(&[0x0F, 0x3A, 0x0F, 0x1B, 0x03]);
    assert_contains(
        &align,
        &[0x0F, 0x3A, 0x0F, 0x1C, 0x24, 0x03],
        "PALIGNR mm3,[rsp],3",
    );
}

#[test]
fn mmx_m64_source_uses_fault_safe_scalar_staging_and_precise_tag_commit() {
    let (_, code) = lower_mmx_m64_instruction(&[0x0F, 0xFC, 0x5B, 0x08]);
    assert_contains(
        &code,
        &[0x48, 0x89, 0x44, 0x24, 0x10],
        "helper result in outer stack slot",
    );
    assert_contains(&code, &[0x0F, 0xFC, 0x1C, 0x24], "PADDB mm3,[rsp]");
    assert_eq!(
        code.windows(5)
            .filter(|window| *window == [0x48, 0x8D, 0x64, 0x24, 0x10])
            .count(),
        2,
        "success and fault paths must each release the outer stack slot"
    );
    super::mmx_helpers::assert_mmx_helper_boundary(&code, "MMX m64-source helper");

    let mut tag_commit = vec![
        0x50,
        0x48,
        0x8B,
        0x45,
        X86_STATE_PTR_AT_RBP as u8,
        0x48,
        0xC7,
        0x80,
    ];
    tag_commit.extend_from_slice(&(X86_GUEST_X87_TAG_WORD_OFFSET as u32).to_le_bytes());
    tag_commit.extend_from_slice(&0u32.to_le_bytes());
    tag_commit.push(0x58);
    assert_contains(&code, &tag_commit, "precise EnterMmx commit");
}
