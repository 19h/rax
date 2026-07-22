//! Fail-closed admission tests for helper-backed scalar MOVBE memory pairs.

use super::*;
use crate::smir::ir::SmirFunction;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::OpId;
use crate::smir::lower::runtime::*;

fn function(direction: X86JitMovbeMemoryDirection, width: OpWidth) -> SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let register = x86(X86Reg::R16);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    match direction {
        X86JitMovbeMemoryDirection::Load => {
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: temporary,
                    addr: Address::Direct(x86(X86Reg::Rbx)),
                    width: width.to_mem_width(),
                    sign: SignExtend::Zero,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::Bswap {
                    dst: register,
                    src: temporary,
                    width,
                },
            );
        }
        X86JitMovbeMemoryDirection::Store => {
            builder.push_op(
                0x1000,
                OpKind::Bswap {
                    dst: temporary,
                    src: register,
                    width,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::Store {
                    src: temporary,
                    addr: Address::Direct(x86(X86Reg::Rbx)),
                    width: width.to_mem_width(),
                },
            );
        }
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn virtual_counts(
    function: &SmirFunction,
) -> (
    std::collections::HashMap<VReg, usize>,
    std::collections::HashMap<VReg, usize>,
) {
    let mut definitions = std::collections::HashMap::new();
    let mut uses = std::collections::HashMap::new();
    for op in &function.blocks[0].ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitMovbeMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_movbe_memory_sequence(&function.blocks[0], 0, allow_mem, &definitions, &uses)
}

fn assert_rejected(function: &SmirFunction) {
    assert!(sequence(function, true).is_none());
    assert!(!is_native_clobber_safe_excluding(
        function,
        &std::collections::HashMap::new(),
        true,
    ));
}

#[test]
fn movbe_memory_gate_admits_only_helper_mode_for_every_width_and_direction() {
    for direction in [
        X86JitMovbeMemoryDirection::Load,
        X86JitMovbeMemoryDirection::Store,
    ] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let function = function(direction, width);
            assert!(sequence(&function, false).is_none());
            assert!(!is_native_clobber_safe(&function));
            assert!(is_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
                true,
            ));
            assert_eq!(
                sequence(&function, true),
                Some(X86JitMovbeMemorySequence {
                    direction,
                    width,
                    consumed: 2,
                })
            );
        }
    }
}

#[test]
fn movbe_memory_gate_rejects_malformed_width_pc_hint_address_register_and_ssa_state() {
    let exact_load = function(X86JitMovbeMemoryDirection::Load, OpWidth::W64);
    let exact_store = function(X86JitMovbeMemoryDirection::Store, OpWidth::W64);
    let mut malformed = Vec::new();

    let mut wrong_width = exact_load.clone();
    let OpKind::Bswap { dst, src, .. } = wrong_width.blocks[0].ops[1].kind.clone() else {
        unreachable!()
    };
    wrong_width.blocks[0].ops[1].kind = OpKind::Bswap {
        dst,
        src,
        width: OpWidth::W32,
    };
    malformed.push(wrong_width);

    let mut wrong_pc = exact_load.clone();
    wrong_pc.blocks[0].ops[1].guest_pc = 0x1001;
    malformed.push(wrong_pc);

    let mut hinted = exact_store.clone();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    malformed.push(hinted);

    let mut signed = exact_load.clone();
    let OpKind::Load {
        dst, addr, width, ..
    } = signed.blocks[0].ops[0].kind.clone()
    else {
        unreachable!()
    };
    signed.blocks[0].ops[0].kind = OpKind::Load {
        dst,
        addr,
        width,
        sign: SignExtend::Sign,
    };
    malformed.push(signed);

    let mut virtual_address = exact_store.clone();
    let OpKind::Store { src, width, .. } = virtual_address.blocks[0].ops[1].kind.clone() else {
        unreachable!()
    };
    virtual_address.blocks[0].ops[1].kind = OpKind::Store {
        src,
        addr: Address::Direct(VReg::Virtual(VirtualId(9))),
        width,
    };
    malformed.push(virtual_address);

    let mut virtual_register = exact_load.clone();
    let OpKind::Bswap { src, width, .. } = virtual_register.blocks[0].ops[1].kind.clone() else {
        unreachable!()
    };
    virtual_register.blocks[0].ops[1].kind = OpKind::Bswap {
        dst: VReg::Virtual(VirtualId(8)),
        src,
        width,
    };
    malformed.push(virtual_register);

    let mut reused = exact_store.clone();
    reused.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        0x1000,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(VReg::Virtual(VirtualId(7))),
            width: OpWidth::W64,
        },
    ));
    malformed.push(reused);

    for function in malformed {
        assert_rejected(&function);
    }
}
