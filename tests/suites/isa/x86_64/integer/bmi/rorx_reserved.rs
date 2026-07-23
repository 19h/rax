//! Reserved-encoding coverage for VEX-encoded RORX.

use crate::common::{CODE_ADDR, setup_vm_no_idt};
use rax::vm::vcpu::{Registers, VCpu};

#[test]
fn rorx_rejects_every_nonreserved_decoded_vvvv_without_committing() {
    for w in 0_u8..=1 {
        for vvvv in 1_u8..=15 {
            let vex3 = (w << 7) | (((!vvvv) & 0x0F) << 3) | 0x03;
            let code = [
                0xC4, 0xE3, vex3, 0xF0, 0xC3, 0x0D, // reserved RORX
                0xF4,
            ];
            let initial = Registers {
                rax: 0xA5A5_5A5A_DEAD_BEEF,
                rbx: 0x0123_4567_89AB_CDEF,
                rflags: 0x2 | 0x8D5,
                ..Registers::default()
            };
            let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial.clone()));

            let error = vcpu
                .step()
                .expect_err("reserved RORX VEX.vvvv must raise #UD")
                .to_string();
            assert!(
                error.contains("IDT entry 6 not present"),
                "W={w}, decoded vvvv={vvvv}: expected #UD, got {error}"
            );

            let after = vcpu.get_regs().unwrap();
            assert_eq!(after.rip, CODE_ADDR, "W={w}, decoded vvvv={vvvv}");
            assert_eq!(after.rax, initial.rax, "W={w}, decoded vvvv={vvvv}");
            assert_eq!(after.rbx, initial.rbx, "W={w}, decoded vvvv={vvvv}");
            assert_eq!(after.rflags, initial.rflags, "W={w}, decoded vvvv={vvvv}");
        }
    }
}
