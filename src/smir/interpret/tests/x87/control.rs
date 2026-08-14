//! Precise x87 no-wait control fault-frontier tests.

use super::*;
use crate::smir::interpret::tests::*;

fn seed(ctx: &mut SmirContext) -> (crate::smir::X86X87State, u64) {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0xA5A5_5A5A_DEAD_BEEF);
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!()
    };
    x86.x87.control_word = 0x027F;
    x86.x87.status_word = (5 << 11) | 0xC7FF;
    x86.x87.tag_word = 0x6996;
    x86.x87.data_ptr = 0x1122_3344_5566_7788;
    x86.x87.instr_ptr = 0x8877_6655_4433_2211;
    x86.x87.last_opcode = 0x05A5;
    x86.x87.regs = std::array::from_fn(|index| {
        let mut raw = [index as u8; 10];
        raw[9] = 0x40 | index as u8;
        raw
    });
    (x86.x87.clone(), ctx.read_vreg(rax))
}

#[test]
fn x87_no_wait_controls_request_exact_direct_replay_before_any_commit() {
    for cr0_fault_bits in [1 << 2, 1 << 3, (1 << 2) | (1 << 3)] {
        for (name, bytes) in [
            ("FNCLEX", &[0xDB, 0xE2][..]),
            ("FNINIT", &[0xDB, 0xE3][..]),
            ("FNSTSW AX", &[0xDF, 0xE0][..]),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            let (x87_before, rax_before) = seed(&mut ctx);
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!()
            };
            x86.cr0 = cr0_fault_bits;

            let result = execute_lifted_x86(bytes, &mut ctx, &mut FlatMemory::new(1));

            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::Undefined {
                        addr: 0x1000,
                        opcode: 0
                    })
                ),
                "{name}, CR0={cr0_fault_bits:#x}: {result:?}"
            );
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.x87, x87_before, "{name}, CR0={cr0_fault_bits:#x}");
            assert_eq!(
                ctx.read_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                rax_before,
                "{name}, CR0={cr0_fault_bits:#x}"
            );
        }
    }
}
