//! Strict lift and canonical interpreter coverage for FSGSBASE.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;

fn base(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn exact_fsgsbase(result: &LiftResult) -> &SmirOp {
    assert_eq!(result.ops.len(), 1);
    result
        .ops
        .first()
        .filter(|op| matches!(op.kind, OpKind::X86FsGsBase { .. }))
        .expect("one exact FSGSBASE semantic op")
}

fn fsgsbase_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict FSGSBASE lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_fsgsbase(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr4 = 1 << 16;
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &fsgsbase_block(bytes),
    );
    (result, context)
}

#[test]
fn fsgsbase_strictly_lifts_every_base_direction_and_width() {
    for (bytes, operand, base_reg, write, width) in [
        (
            &[0xF3, 0x0F, 0xAE, 0xC0][..],
            0,
            X86Reg::FsBase,
            false,
            OpWidth::W32,
        ),
        (
            &[0xF3, 0x48, 0x0F, 0xAE, 0xC9],
            1,
            X86Reg::GsBase,
            false,
            OpWidth::W64,
        ),
        (
            &[0xF3, 0x0F, 0xAE, 0xD2],
            2,
            X86Reg::FsBase,
            true,
            OpWidth::W32,
        ),
        (
            &[0xF3, 0x48, 0x0F, 0xAE, 0xDB],
            3,
            X86Reg::GsBase,
            true,
            OpWidth::W64,
        ),
    ] {
        let result = lift_single(bytes).expect("valid FSGSBASE encoding");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_fsgsbase(&result).kind,
            OpKind::X86FsGsBase {
                operand: got_operand,
                base: got_base,
                write: got_write,
                width: got_width,
                requires_apx: false,
            } if got_operand == x86_gpr(operand)
                && got_base == base(base_reg)
                && got_write == write
                && got_width == width
        ));
    }
}

#[test]
fn fsgsbase_lifts_rex_and_rex2_register_extensions_exactly() {
    for (bytes, operand, base_reg, write, width, requires_apx) in [
        (
            &[0xF3, 0x49, 0x0F, 0xAE, 0xC0][..],
            8,
            X86Reg::FsBase,
            false,
            OpWidth::W64,
            false,
        ),
        (
            &[0xF3, 0xD5, 0x90, 0xAE, 0xC0],
            16,
            X86Reg::FsBase,
            false,
            OpWidth::W32,
            true,
        ),
        (
            &[0xF3, 0xD5, 0x98, 0xAE, 0xD0],
            16,
            X86Reg::FsBase,
            true,
            OpWidth::W64,
            true,
        ),
        (
            &[0xF3, 0xD5, 0x99, 0xAE, 0xCF],
            31,
            X86Reg::GsBase,
            false,
            OpWidth::W64,
            true,
        ),
        (
            &[0xF3, 0xD5, 0x99, 0xAE, 0xDF],
            31,
            X86Reg::GsBase,
            true,
            OpWidth::W64,
            true,
        ),
    ] {
        let result = lift_single(bytes).expect("valid extended FSGSBASE encoding");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            exact_fsgsbase(&result).kind,
            OpKind::X86FsGsBase {
                operand: got_operand,
                base: got_base,
                write: got_write,
                width: got_width,
                requires_apx: got_apx,
            } if got_operand == x86_gpr(operand)
                && got_base == base(base_reg)
                && got_write == write
                && got_width == width
                && got_apx == requires_apx
        ));
    }
}

#[test]
fn fsgsbase_enforces_w32_w64_and_reserved_encoding_boundaries() {
    assert_invalid_opcode_trap(
        &lift_single(&[0x66, 0xF3, 0x0F, 0xAE, 0xC0])
            .expect("reserved FSGSBASE W16 form must strictly lift"),
        5,
    );
    let with_rex_w = lift_single(&[0x66, 0xF3, 0x48, 0x0F, 0xAE, 0xC0])
        .expect("REX.W overrides 66h for FSGSBASE");
    assert!(matches!(
        exact_fsgsbase(&with_rex_w).kind,
        OpKind::X86FsGsBase {
            width: OpWidth::W64,
            ..
        }
    ));

    assert_invalid_opcode_trap(
        &lift_single(&[0xF0, 0xF3, 0x0F, 0xAE, 0xC0])
            .expect("LOCK FSGSBASE must strictly lift to #UD"),
        5,
    );

    // F3 is an ignored legacy prefix on memory-form FXSAVE; ModR/M.mod, not
    // the prefix alone, distinguishes that instruction from RDFSBASE.
    let fxsave = lift_single(&[0xF3, 0x0F, 0xAE, 0x00]).expect("prefixed FXSAVE remains valid");
    assert!(matches!(
        fxsave.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FxSave { .. },
            ..
        }]
    ));
    for missing_f3 in [&[0x0F, 0xAE, 0xC0][..], &[0xF2, 0x0F, 0xAE, 0xD0]] {
        let result = lift_single(missing_f3)
            .expect("reserved non-F3 Group-15 register form must strictly lift");
        assert_invalid_opcode_trap(&result, missing_f3.len());
    }

    for accepted in [
        &[0x67, 0xF3, 0x0F, 0xAE, 0xC0][..],
        &[0x64, 0xF3, 0x0F, 0xAE, 0xC0],
    ] {
        assert_eq!(
            lift_single(accepted).unwrap().bytes_consumed,
            accepted.len()
        );
    }
}

#[test]
fn fsgsbase_metadata_tracks_conditional_source_and_destination_contracts() {
    let read = exact_fsgsbase(&lift_single(&[0xF3, 0x0F, 0xAE, 0xC8]).unwrap()).clone();
    assert_eq!(read.kind.source_vregs(), vec![base(X86Reg::GsBase)]);
    assert_eq!(read.kind.dests(), vec![x86_gpr(0)]);
    assert!(read.kind.has_side_effects());
    assert!(read.is_jit_safe());

    let write = exact_fsgsbase(&lift_single(&[0xF3, 0x0F, 0xAE, 0xD0]).unwrap()).clone();
    assert_eq!(write.kind.source_vregs(), vec![x86_gpr(0)]);
    assert_eq!(write.kind.dests(), vec![base(X86Reg::FsBase)]);
    assert!(write.kind.has_side_effects());
    assert!(write.is_jit_safe());
}

#[test]
fn fsgsbase_interpreter_reads_with_exact_w32_w64_writeback_and_preserves_flags() {
    let flags = MaterializedFlags {
        cf: true,
        zf: true,
        sf: true,
        of: true,
        pf: true,
        af: true,
        df: true,
        ac: true,
    };
    let (result, context) = execute_fsgsbase(&[0xF3, 0x0F, 0xAE, 0xC0], |context| {
        context.flags.materialized = flags;
        context.write_vreg(base(X86Reg::FsBase), 0xFFFF_8000_89AB_CDEF);
        context.write_vreg(x86_gpr(0), u64::MAX);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86_gpr(0)), 0x89AB_CDEF);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    assert!(context.flags.lazy.is_none());

    let (result, context) = execute_fsgsbase(&[0xF3, 0x48, 0x0F, 0xAE, 0xC8], |context| {
        context.write_vreg(base(X86Reg::GsBase), 0xFFFF_8000_89AB_CDEF);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86_gpr(0)), 0xFFFF_8000_89AB_CDEF);
}

#[test]
fn fsgsbase_interpreter_writes_zero_extended_w32_and_canonical_w64_values() {
    let (result, context) = execute_fsgsbase(&[0xF3, 0x0F, 0xAE, 0xD0], |context| {
        context.write_vreg(x86_gpr(0), 0xFFFF_FFFF_89AB_CDEF);
        context.write_vreg(base(X86Reg::FsBase), u64::MAX);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(base(X86Reg::FsBase)), 0x89AB_CDEF);

    for value in [0, 0x0000_7FFF_FFFF_FFFF, 0xFFFF_8000_0000_0000, u64::MAX] {
        let (result, context) = execute_fsgsbase(&[0xF3, 0x48, 0x0F, 0xAE, 0xD8], |context| {
            context.write_vreg(x86_gpr(0), value);
            context.write_vreg(base(X86Reg::GsBase), 0x1234);
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(context.read_vreg(base(X86Reg::GsBase)), value);
    }
}

#[test]
fn fsgsbase_interpreter_faults_before_any_destination_or_base_commit() {
    let (result, context) = execute_fsgsbase(&[0xF3, 0x0F, 0xAE, 0xC0], |context| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr4 = 0;
        context.write_vreg(x86_gpr(0), 0xA5A5);
        context.write_vreg(base(X86Reg::FsBase), 0x1234);
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
    assert_eq!(context.read_vreg(x86_gpr(0)), 0xA5A5);

    for value in [0x0000_8000_0000_0000, 0xFFFF_7FFF_FFFF_FFFF] {
        let (result, context) = execute_fsgsbase(&[0xF3, 0x48, 0x0F, 0xAE, 0xD8], |context| {
            context.write_vreg(x86_gpr(0), value);
            context.write_vreg(base(X86Reg::GsBase), 0x1234);
        });
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0
            })
        ));
        assert_eq!(context.read_vreg(base(X86Reg::GsBase)), 0x1234);
    }

    let rex2 = [0xF3, 0xD5, 0x98, 0xAE, 0xC0];
    let (result, context) = execute_fsgsbase(&rex2, |context| {
        context.write_vreg(x86_gpr(16), 0xA5A5);
        context.write_vreg(base(X86Reg::FsBase), 0x1234);
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { .. })
    ));
    assert_eq!(context.read_vreg(x86_gpr(16)), 0xA5A5);

    let (result, context) = execute_fsgsbase(&rex2, |context| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = true;
        context.write_vreg(base(X86Reg::FsBase), 0x1234);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86_gpr(16)), 0x1234);
}

#[test]
fn fsgsbase_o2_retains_faults_and_exact_encoded_register_operands() {
    let mut block = fsgsbase_block(&[0xF3, 0xD5, 0x98, 0xAE, 0xD0]); // WRFSBASE R16
    block.ops.insert(
        0,
        SmirOp::new(
            OpId(1),
            0x0FFF,
            OpKind::Mov {
                dst: x86_gpr(16),
                src: SrcOperand::Reg(x86_gpr(0)),
                width: OpWidth::W64,
            },
        ),
    );
    block.ops.push(SmirOp::new(
        OpId(2),
        0x1005,
        OpKind::Mov {
            dst: base(X86Reg::FsBase),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .any(|op| matches!(
                op.kind,
                OpKind::X86FsGsBase {
                    operand,
                    base: base_reg,
                    write: true,
                    width: OpWidth::W64,
                    requires_apx: true,
                } if operand == x86_gpr(16) && base_reg == base(X86Reg::FsBase)
            ))
    );
}
