//! EVEX saturating-pack memory-fault tests.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn evex_saturating_packs_use_e4nf_complete_memory_accesses() {
    let mut context = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    context.write_vreg(rax, 0x1000);

    // Intel SDM exception classes E4NF/E4NF.nb do not suppress the r/m
    // access under an empty or sparse destination writemask. Full-vector
    // forms access 16/32/64 bytes; broadcast forms access one 4-byte scalar.
    for instruction in [
        &[0x62, 0xF1, 0x75, 0x49, 0x63, 0x00][..],
        &[0x62, 0xF1, 0x75, 0x49, 0x67, 0x00][..],
        &[0x62, 0xF1, 0x75, 0x49, 0x6B, 0x00][..],
        &[0x62, 0xF2, 0x75, 0x49, 0x2B, 0x00][..],
        &[0x62, 0xF1, 0x75, 0x59, 0x6B, 0x00][..],
        &[0x62, 0xF2, 0x75, 0x59, 0x2B, 0x00][..],
    ] {
        for mask in [0, 1, u64::MAX] {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.xmm[0] = sentinel;
            x86.xmm[1] = [0; 16];
            context.write_vreg(k1, mask);

            let result = execute_lifted_x86(instruction, &mut context, &mut memory);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "E4NF source access was suppressed for {instruction:02X?}, \
                 mask={mask:#018X}: {result:?}"
            );
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!()
            };
            assert_eq!(
                x86.xmm[0], sentinel,
                "faulting E4NF source committed {instruction:02X?}, mask={mask:#018X}"
            );
        }
    }
}
