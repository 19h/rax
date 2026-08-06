//! Native MOVQ2DQ/MOVDQ2Q encoding, state staging, and execution.

use super::*;
use crate::smir::ir::types::VirtualId;

fn mm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Mm(index)))
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn kind(dst: VReg, src: VReg) -> OpKind {
    OpKind::X86MovdQ {
        dst,
        src,
        width: OpWidth::W64,
        zero_upper: false,
    }
}

fn hint(prefix: X86SsePrefix) -> X86OpHint {
    X86OpHint::SseOp {
        prefix,
        opcode: 0xD6,
    }
}

fn expected_transfer(xmm_index: u8, mm_index: u8, xmm_destination: bool) -> Vec<u8> {
    let mut bytes = vec![if xmm_destination { 0xF3 } else { 0xF2 }];
    if xmm_index >= 8 {
        bytes.push(if xmm_destination { 0x44 } else { 0x41 });
    }
    bytes.extend_from_slice(&[
        0x0F,
        0xD6,
        0xC0 | if xmm_destination {
            (xmm_index & 7) << 3 | mm_index
        } else {
            mm_index << 3 | (xmm_index & 7)
        },
    ]);
    bytes
}

fn expected_state_move(xmm_index: u8, opcode: u8) -> Vec<u8> {
    let mut bytes = vec![0xF3];
    if xmm_index >= 8 {
        bytes.push(0x44);
    }
    bytes.extend_from_slice(&[0x0F, opcode, 0x80 | ((xmm_index & 7) << 3)]);
    bytes.extend_from_slice(&(X86_GUEST_ZMM_OFFSET + i32::from(xmm_index) * 64).to_le_bytes());
    bytes
}

fn contains(bytes: &[u8], expected: &[u8]) -> bool {
    bytes
        .windows(expected.len())
        .any(|window| window == expected)
}

#[test]
fn every_register_pair_emits_exact_legacy_opcode_and_minimal_state_sync() {
    let mut probes = 0usize;
    for xmm_index in 0..16 {
        for mm_index in 0..8 {
            for xmm_destination in [false, true] {
                let (operation, prefix, state_opcode) = if xmm_destination {
                    (kind(xmm(xmm_index), mm(mm_index)), X86SsePrefix::Rep, 0x7F)
                } else {
                    (
                        kind(mm(mm_index), xmm(xmm_index)),
                        X86SsePrefix::Repne,
                        0x6F,
                    )
                };
                let code = lower_single_hinted_op(operation, hint(prefix));
                let transfer = expected_transfer(xmm_index, mm_index, xmm_destination);
                let state = expected_state_move(xmm_index, state_opcode);
                assert!(
                    contains(&code, &transfer),
                    "missing transfer {transfer:02X?} in {code:02X?}"
                );
                assert!(
                    contains(&code, &state),
                    "missing state sync {state:02X?} in {code:02X?}"
                );
                assert_eq!(
                    code.windows(state.len())
                        .filter(|window| *window == state)
                        .count(),
                    1,
                    "state sync must occur exactly once: {code:02X?}"
                );
                probes += 1;
            }
        }
    }
    assert_eq!(probes, 16 * 8 * 2);
}

#[test]
fn live_native_vector_state_uses_direct_xmm_without_stale_slot_staging() {
    for (operation, prefix, xmm_index, mm_index, xmm_destination) in [
        (kind(xmm(15), mm(7)), X86SsePrefix::Rep, 15, 7, true),
        (kind(mm(7), xmm(15)), X86SsePrefix::Repne, 15, 7, false),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, operation);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint(prefix));

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_native_vector_state_active(true);
        lowerer.lower_function(&function).unwrap();
        let code = lowerer.finalize().unwrap();
        let transfer = expected_transfer(xmm_index, mm_index, xmm_destination);
        assert!(contains(&code, &transfer));
        assert!(!contains(&code, &expected_state_move(xmm_index, 0x6F)));
        assert!(!contains(&code, &expected_state_move(xmm_index, 0x7F)));
    }
}

#[test]
fn malformed_width_hint_and_register_files_never_lower_as_cross_transfers() {
    for (operation, operation_hint) in [
        (
            OpKind::X86MovdQ {
                dst: xmm(15),
                src: mm(7),
                width: OpWidth::W32,
                zero_upper: false,
            },
            hint(X86SsePrefix::Rep),
        ),
        (
            OpKind::X86MovdQ {
                dst: xmm(15),
                src: mm(7),
                width: OpWidth::W64,
                zero_upper: true,
            },
            hint(X86SsePrefix::Rep),
        ),
        (kind(xmm(15), mm(7)), hint(X86SsePrefix::Repne)),
        (kind(mm(7), xmm(15)), hint(X86SsePrefix::Rep)),
        (kind(xmm(16), mm(7)), hint(X86SsePrefix::Rep)),
        (kind(mm(8), xmm(15)), hint(X86SsePrefix::Repne)),
        (
            kind(VReg::Virtual(VirtualId(1)), mm(7)),
            hint(X86SsePrefix::Rep),
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(operation, operation_hint),
            LowerError::InvalidOperand { .. }
                | LowerError::InvalidRegister(_)
                | LowerError::UnsupportedOp { .. }
        ));
    }
}

fn native_function() -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.push_op(0x1000, kind(mm(7), xmm(14)));
    builder.push_op(
        0x1005,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.push_op(0x1005, kind(xmm(15), mm(3)));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[1].x86_hint = Some(hint(X86SsePrefix::Repne));
    function.blocks[0].ops[3].x86_hint = Some(hint(X86SsePrefix::Rep));
    function
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_sse2_execution_matches_state_backed_cross_file_semantics_at_all_levels() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O1,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let mut function = native_function();
        crate::smir::optimize::optimize_function(&mut function, level);
        let excluded = std::collections::HashMap::new();
        assert!(
            crate::smir::lower::runtime::is_native_clobber_safe_excluding(
                &function, &excluded, false
            )
        );
        assert!(
            crate::smir::lower::runtime::x86_native_mmx_pairs_valid_excluding(&function, &excluded)
        );
        assert!(crate::smir::lower::runtime::uses_x86_xmm_state_excluding(
            &function, &excluded
        ));
        assert!(
            !crate::smir::lower::runtime::uses_x86_native_vectors_excluding(&function, &excluded)
        );

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&function)
            .unwrap_or_else(|error| panic!("lower after {level:?}: {error:?}"));
        let code = lowerer.finalize().unwrap();
        let exec = ExecMem::new(&code).unwrap();

        let initial_mm = [
            0x0000_0000_0000_0000,
            0x1111_1111_1111_1111,
            0x2222_2222_2222_2222,
            0x0123_4567_89AB_CDEF,
            0x4444_4444_4444_4444,
            0x5555_5555_5555_5555,
            0x6666_6666_6666_6666,
            0x7777_7777_7777_7777,
        ];
        let initial_zmm = std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xA000_0000_0000_0000u64 | ((register as u64) << 32) | word as u64
            })
        });
        let mut regs = GuestRegs {
            gpr: std::array::from_fn(|index| 0x8000_0000_0000_0000 | index as u64),
            rflags: 0x2 | 0x08D5,
            zmm: initial_zmm,
            xmm_state_active: 1,
            mm: initial_mm,
            mmx_active: 1,
            x87_tag_word: 0xFFFF,
            ..GuestRegs::default()
        };
        let initial_gpr = regs.gpr;
        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.mm[7], initial_zmm[14][0], "{level:?}");
        assert_eq!(&regs.mm[..7], &initial_mm[..7], "{level:?}");
        assert_eq!(regs.zmm[14], initial_zmm[14], "{level:?}");
        assert_eq!(regs.zmm[15][0], initial_mm[3], "{level:?}");
        assert_eq!(regs.zmm[15][1], 0, "{level:?}");
        assert_eq!(&regs.zmm[15][2..], &initial_zmm[15][2..], "{level:?}");
        for index in 0..14 {
            assert_eq!(regs.zmm[index], initial_zmm[index], "{level:?}, ZMM{index}");
        }
        assert_eq!(regs.gpr, initial_gpr, "{level:?}");
        assert_eq!(regs.rflags & 0x08D5, 0x08D5, "{level:?}");
        assert_eq!(regs.x87_tag_word, 0, "{level:?}");
    }
}
