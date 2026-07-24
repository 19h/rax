//! x86 vector floating-point comparison exception-precedence tests.

use super::*;

fn f32_vector(values: [u32; 4]) -> [u64; 16] {
    let bytes = values
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut vector = [0u64; 16];
    for (word, chunk) in bytes.chunks_exact(8).enumerate() {
        vector[word] = u64::from_le_bytes(chunk.try_into().unwrap());
    }
    vector
}

#[test]
fn vector_compare_nan_precedes_same_lane_denormal_but_not_other_lanes() {
    const DENORMAL: u32 = 0x0000_0001;
    const SNAN: u32 = 0x7F80_0001;
    const ONE: u32 = 0x3F80_0000;
    // EVEX.128 VCMPPS k1{k1}, xmm1, xmm3, NLE_UQ. This is the exact
    // predicate/control shape that exposed the native differential.
    const BYTES: [u8; 7] = [0x62, 0xF1, 0x74, 0x09, 0xC2, 0xCB, 0x16];

    let mut context = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(1);

    // The SNaN in the same active lane suppresses that lane's lower-priority
    // denormal exception. Only MXCSR.IE becomes sticky.
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.xmm[1] = f32_vector([DENORMAL, 0, 0, 0]);
        x86.xmm[3] = f32_vector([SNAN, 0, 0, 0]);
        x86.k[1] = 0b0001;
        x86.mxcsr = 0x1F80;
    }
    assert!(matches!(
        execute_lifted_x86(&BYTES, &mut context, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.mxcsr & 0x3F, 1);

    // A separate active non-NaN lane still contributes DE, so packed status
    // reduction retains both IE and DE across independent lanes.
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.xmm[1] = f32_vector([DENORMAL, ONE, 0, 0]);
        x86.xmm[3] = f32_vector([ONE, SNAN, 0, 0]);
        x86.k[1] = 0b0011;
        x86.mxcsr = 0x1F80;
    }
    assert!(matches!(
        execute_lifted_x86(&BYTES, &mut context, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.mxcsr & 0x3F, 0x3);
}
