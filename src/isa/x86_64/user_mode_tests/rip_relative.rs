//! Legacy SIMD addressing regressions. Intel SDM Vol. 2A section 2.2.1.6
//! defines the base as the next instruction, including trailing immediates.
//! The operand's distinct neighboring bytes expose a one-byte address error.

use super::*;
use crate::smir::interpret::SmirInterpreter;
use crate::smir::ir::context::{ArchRegState, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::types::SourceArch;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};

const INITIAL: [u64; 2] = [0x1716_1514_1312_1110, 0x1f1e_1d1c_1b1a_1918];
const RAMP: [u64; 2] = [0x4746_4544_4342_4140, 0x4f4e_4d4c_4b4a_4948];

/// `head` ends with ModR/M mod=00,r/m=101. Displacement arithmetic is
/// signed 32-bit bytes relative to the complete instruction's end.
fn instruction(head: &[u8], immediate: u8, target: u64) -> Vec<u8> {
    let end = CODE + (head.len() + 4 + 1) as u64;
    let displacement = i32::try_from(target as i64 - end as i64).unwrap();
    let mut code = head.to_vec();
    code.extend_from_slice(&displacement.to_le_bytes());
    code.push(immediate);
    code
}

/// Exercise positive/negative displacements, 64-/32-bit address sizes, and
/// both freshly decoded and cached execution of the same instruction.
fn check_load(head: &[u8], immediate: u8, expected: [u64; 2]) {
    for target in [DATA, CODE - 0x1000 + 0x800] {
        for addr32 in [false, true] {
            let mut prefixed = Vec::new();
            if addr32 {
                prefixed.push(0x67);
            }
            prefixed.extend_from_slice(head);
            let code = instruction(&prefixed, immediate, target);
            let mut h = harness(&code);
            let frame = if target == DATA {
                0x1000
            } else {
                h.space.map(CODE - 0x1000, 0x4000, R);
                0x4800
            };
            let ramp: Vec<u8> = (0x3f..=0x50).collect();
            h.mem.write_slice(&ramp, GuestAddress(frame - 1)).unwrap();
            for cached in [false, true] {
                h.vcpu.regs.rip = CODE;
                h.vcpu.regs.xmm[0] = INITIAL;
                assert!(
                    h.vcpu.step_with_faults().unwrap().is_none(),
                    "{code:02x?}, target={target:#x}, cached={cached}"
                );
                assert_eq!(h.vcpu.regs.rip, CODE + code.len() as u64);
                assert_eq!(
                    h.vcpu.regs.xmm[0], expected,
                    "{code:02x?}, target={target:#x}, cached={cached}"
                );
            }

            // The independent SMIR decoder already uses the full length.
            // Check execution parity as well as the architectural expectation.
            let mut lifter = X86_64Lifter::strict();
            let mut lift_context = LiftContext::new(SourceArch::X86_64);
            let lifted = lifter.lift_insn(CODE, &code, &mut lift_context).unwrap();
            assert_eq!(lifted.bytes_consumed, code.len());
            let mut context = SmirContext::new_x86_64();
            context.pc = CODE;
            let ArchRegState::X86_64(registers) = &mut context.arch_regs else {
                unreachable!();
            };
            registers.xmm[0][..2].copy_from_slice(&INITIAL);
            let mut memory = FlatMemory::with_base(target - 1, ramp.len());
            memory.load(0, &ramp);
            let interpreter = SmirInterpreter::new();
            for op in &lifted.ops {
                interpreter
                    .execute_op(&mut context, &mut memory, op)
                    .unwrap();
                assert!(
                    context.exit_reason.is_none(),
                    "{code:02x?}: {:?}",
                    context.exit_reason
                );
            }
            let ArchRegState::X86_64(registers) = &context.arch_regs else {
                unreachable!();
            };
            assert_eq!(registers.xmm[0][..2], expected, "SMIR: {code:02x?}");
        }
    }
}

#[test]
fn legacy_0f_immediates_follow_rip_relative_sources() {
    // PSHUFD/HW/LW identity permutations.
    for prefix in [0x66, 0xf3, 0xf2] {
        check_load(&[prefix, 0x0f, 0x70, 0x05], 0xe4, RAMP);
    }
    // PINSRW inserts the low memory word into word 7.
    check_load(
        &[0x66, 0x0f, 0xc4, 0x05],
        7,
        [INITIAL[0], 0x4140_1d1c_1b1a_1918],
    );
    // SHUFPS/SHUFPD choose low elements from the old destination and memory.
    check_load(&[0x0f, 0xc6, 0x05], 0x44, [INITIAL[0], RAMP[0]]);
    check_load(&[0x66, 0x0f, 0xc6, 0x05], 0, [INITIAL[0], RAMP[0]]);
    // CMPNEQPS/PD: every initial lane differs from its memory source.
    check_load(&[0x0f, 0xc2, 0x05], 4, [u64::MAX; 2]);
    check_load(&[0x66, 0x0f, 0xc2, 0x05], 4, [u64::MAX; 2]);
}

#[test]
fn legacy_0f3a_immediates_follow_rip_relative_sources() {
    check_load(
        &[0x66, 0x0f, 0x3a, 0x20, 0x05],
        15,
        [INITIAL[0], 0x401e_1d1c_1b1a_1918],
    );
    check_load(
        &[0x66, 0x0f, 0x3a, 0x22, 0x05],
        3,
        [INITIAL[0], 0x4342_4140_1b1a_1918],
    );
    check_load(
        &[0x66, 0x48, 0x0f, 0x3a, 0x22, 0x05],
        1,
        [INITIAL[0], RAMP[0]],
    );
    // PALIGNR with zero selects the entire memory source.
    check_load(&[0x66, 0x0f, 0x3a, 0x0f, 0x05], 0, RAMP);
}

#[test]
fn legacy_0f3a_immediates_follow_rip_relative_destinations() {
    for (head, width, value) in [
        (&[0x66, 0x0f, 0x3a, 0x14, 0x05][..], 1, 0x11_u64),
        (&[0x66, 0x0f, 0x3a, 0x15, 0x05][..], 2, 0x1312),
        (&[0x66, 0x0f, 0x3a, 0x16, 0x05][..], 4, 0x1716_1514),
        (&[0x66, 0x48, 0x0f, 0x3a, 0x16, 0x05][..], 8, INITIAL[1]),
    ] {
        let code = instruction(head, 1, DATA + 0x100);
        let mut h = harness(&code);
        h.mem
            .write_slice(&[0xa5; 10], GuestAddress(0x10ff))
            .unwrap();
        for cached in [false, true] {
            h.vcpu.regs.rip = CODE;
            h.vcpu.regs.xmm[0] = INITIAL;
            assert!(h.vcpu.step_with_faults().unwrap().is_none());
            let mut actual = [0; 10];
            h.mem.read_slice(&mut actual, GuestAddress(0x10ff)).unwrap();
            let mut expected = [0xa5; 10];
            expected[1..1 + width].copy_from_slice(&value.to_le_bytes()[..width]);
            assert_eq!(actual, expected, "{code:02x?}, cached={cached}");
        }
    }
}

#[test]
fn pin_style_rip_relative_pointer_table_reaches_the_call_target() {
    let target = CODE + 0x300;
    let mut code = instruction(&[0x66, 0x48, 0x0f, 0x3a, 0x22, 0x05], 1, DATA + 0x100);
    // PEXTRQ rax,xmm0,1; CALL rax; SYSCALL. The callee is a RET.
    code.extend_from_slice(&[
        0x66, 0x48, 0x0f, 0x3a, 0x16, 0xc0, 1, 0xff, 0xd0, 0x0f, 0x05,
    ]);
    let mut h = harness(&code);
    h.mem
        .write_slice(&target.to_le_bytes(), GuestAddress(0x1100))
        .unwrap();
    h.mem.write_slice(&[0xc3], GuestAddress(0x300)).unwrap();
    for cached in [false, true] {
        h.vcpu.regs.rip = CODE;
        let stack = h.vcpu.regs.rsp;
        assert!(
            matches!(run(&mut h.vcpu), Ok(VcpuExit::SystemCall)),
            "cached={cached}"
        );
        assert_eq!(h.vcpu.regs.rax, target);
        assert_eq!(h.vcpu.regs.rip, CODE + code.len() as u64);
        assert_eq!(h.vcpu.regs.rsp, stack);
        assert!(matches!(
            h.vcpu.take_user_trap(),
            Some(X86UserTrap::SystemCall { .. })
        ));
    }
}
