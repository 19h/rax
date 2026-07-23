//! Canonical interpreter and optimizer parity for AVX-512 opmask operations.

use super::*;
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::SmirMemory;
use crate::smir::ir::ops::{
    X86OpmaskBinaryKind, X86OpmaskMoveDestination, X86OpmaskMoveSource, X86OpmaskOp,
    X86OpmaskShiftKind, X86OpmaskTestKind,
};
use crate::smir::optimize::{OptLevel, optimize_function};

const INITIAL_RFLAGS: u64 = 0x2 | 0x08D5;

fn k(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::K(index)))
}

fn gpr(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn context() -> SmirContext {
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(INITIAL_RFLAGS);
    ctx.flags.lazy = None;
    ctx
}

fn execute(
    opmask: X86OpmaskOp,
    level: OptLevel,
    ctx: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x2345, OpKind::X86Opmask(opmask));
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    optimize_function(&mut function, level);
    SmirInterpreter::new().execute_block(ctx, memory, &function.blocks[0])
}

fn assert_halt(exit: BlockResult) {
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
}

fn read_bytes(memory: &mut dyn SmirMemory, addr: u64, size: usize) -> Vec<u8> {
    let mut bytes = vec![0; size];
    memory.read(addr, &mut bytes).unwrap();
    bytes
}

#[test]
fn binary_not_unpack_and_shift_match_exact_width_semantics_at_o0_o1_o2() {
    let lhs = 0xFEDC_BA98_7654_32F1_u64;
    let rhs = 0x1234_5678_9ABC_DEF3_u64;

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let mask = width.mask();
            for (kind, expected) in [
                (X86OpmaskBinaryKind::Add, lhs.wrapping_add(rhs)),
                (X86OpmaskBinaryKind::And, lhs & rhs),
                (X86OpmaskBinaryKind::AndNot, !lhs & rhs),
                (X86OpmaskBinaryKind::Or, lhs | rhs),
                (X86OpmaskBinaryKind::Xnor, !(lhs ^ rhs)),
                (X86OpmaskBinaryKind::Xor, lhs ^ rhs),
            ] {
                let mut ctx = context();
                ctx.write_vreg(k(1), lhs);
                ctx.write_vreg(k(2), rhs);
                ctx.write_vreg(k(3), u64::MAX);
                assert_halt(execute(
                    X86OpmaskOp::Binary {
                        kind,
                        dst: k(3),
                        src1: k(1),
                        src2: k(2),
                        width,
                    },
                    level,
                    &mut ctx,
                    &mut FlatMemory::new(1),
                ));
                assert_eq!(ctx.read_vreg(k(3)), expected & mask, "{kind:?} {width:?}");
                assert_eq!(ctx.read_vreg(k(1)), lhs, "source 1 {kind:?} {width:?}");
                assert_eq!(ctx.read_vreg(k(2)), rhs, "source 2 {kind:?} {width:?}");
                assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_RFLAGS);
            }

            let mut ctx = context();
            ctx.write_vreg(k(1), lhs);
            ctx.write_vreg(k(3), u64::MAX);
            assert_halt(execute(
                X86OpmaskOp::Not {
                    dst: k(3),
                    src: k(1),
                    width,
                },
                level,
                &mut ctx,
                &mut FlatMemory::new(1),
            ));
            assert_eq!(ctx.read_vreg(k(3)), !lhs & mask, "KNOT {width:?}");
            assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_RFLAGS);

            for count in [0, (width.bits() - 1) as u8, width.bits() as u8, 0xFF] {
                for kind in [X86OpmaskShiftKind::Left, X86OpmaskShiftKind::Right] {
                    let mut ctx = context();
                    ctx.write_vreg(k(1), lhs);
                    ctx.write_vreg(k(3), u64::MAX);
                    assert_halt(execute(
                        X86OpmaskOp::Shift {
                            kind,
                            dst: k(3),
                            src: k(1),
                            width,
                            count,
                        },
                        level,
                        &mut ctx,
                        &mut FlatMemory::new(1),
                    ));
                    let expected = if u32::from(count) >= width.bits() {
                        0
                    } else if kind == X86OpmaskShiftKind::Left {
                        (lhs & mask) << count
                    } else {
                        (lhs & mask) >> count
                    };
                    assert_eq!(
                        ctx.read_vreg(k(3)),
                        expected & mask,
                        "KSHIFT {kind:?} {width:?} count={count}"
                    );
                    assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_RFLAGS);
                }
            }
        }

        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let half_bits = width.bits() / 2;
            let half_mask = (1_u64 << half_bits) - 1;
            let mut ctx = context();
            ctx.write_vreg(k(1), lhs);
            ctx.write_vreg(k(2), rhs);
            assert_halt(execute(
                X86OpmaskOp::Unpack {
                    dst: k(3),
                    src1: k(1),
                    src2: k(2),
                    width,
                },
                level,
                &mut ctx,
                &mut FlatMemory::new(1),
            ));
            assert_eq!(
                ctx.read_vreg(k(3)),
                ((lhs & half_mask) << half_bits) | (rhs & half_mask),
                "KUNPCK {width:?}"
            );
        }
    }
}

#[test]
fn kmov_zero_extends_every_destination_class_and_transfers_exact_memory_widths() {
    let value = 0xFEDC_BA98_7654_3210_u64;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let expected = value & width.mask();

            for src in [
                X86OpmaskMoveSource::Mask(k(1)),
                X86OpmaskMoveSource::Gpr(gpr(X86Reg::R8)),
            ] {
                let mut ctx = context();
                ctx.write_vreg(k(1), value);
                ctx.write_vreg(gpr(X86Reg::R8), value);
                ctx.write_vreg(k(2), u64::MAX);
                assert_halt(execute(
                    X86OpmaskOp::MoveToMask {
                        dst: k(2),
                        src,
                        width,
                    },
                    level,
                    &mut ctx,
                    &mut FlatMemory::new(1),
                ));
                assert_eq!(ctx.read_vreg(k(2)), expected, "KMOV to K {width:?}");
            }

            let mut ctx = context();
            ctx.write_vreg(k(1), value);
            ctx.write_vreg(gpr(X86Reg::R9), u64::MAX);
            assert_halt(execute(
                X86OpmaskOp::MoveFromMask {
                    dst: X86OpmaskMoveDestination::Gpr(gpr(X86Reg::R9)),
                    src: k(1),
                    width,
                },
                level,
                &mut ctx,
                &mut FlatMemory::new(1),
            ));
            assert_eq!(
                ctx.read_vreg(gpr(X86Reg::R9)),
                expected,
                "KMOV from K {width:?}"
            );

            let bytes = width.bytes() as usize;
            let mut memory = FlatMemory::new(0x40);
            memory.load(0x10, &value.to_le_bytes());
            let mut ctx = context();
            ctx.write_vreg(k(2), u64::MAX);
            assert_halt(execute(
                X86OpmaskOp::MoveToMask {
                    dst: k(2),
                    src: X86OpmaskMoveSource::Memory(Address::Absolute(0x10)),
                    width,
                },
                level,
                &mut ctx,
                &mut memory,
            ));
            assert_eq!(ctx.read_vreg(k(2)), expected, "KMOV load {width:?}");

            let sentinel = [0xA5; 16];
            memory.load(0x20, &sentinel);
            ctx.write_vreg(k(1), value);
            assert_halt(execute(
                X86OpmaskOp::MoveFromMask {
                    dst: X86OpmaskMoveDestination::Memory(Address::Absolute(0x24)),
                    src: k(1),
                    width,
                },
                level,
                &mut ctx,
                &mut memory,
            ));
            assert_eq!(
                read_bytes(&mut memory, 0x24, bytes),
                value.to_le_bytes()[..bytes]
            );
            assert_eq!(read_bytes(&mut memory, 0x20, 4), vec![0xA5; 4]);
            assert_eq!(
                read_bytes(&mut memory, 0x24 + bytes as u64, 12 - bytes),
                vec![0xA5; 12 - bytes],
                "KMOV store overrun {width:?}"
            );
            assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_RFLAGS);
        }
    }
}

#[test]
fn kmov_memory_faults_are_precise_and_noncommitting_at_o0_o1_o2() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let bytes = width.bytes() as usize;
            let fault_addr = if bytes == 1 {
                0x20
            } else {
                0x20 - (bytes / 2) as u64
            };
            let reported_addr = if bytes == 1 {
                fault_addr
            } else {
                fault_addr + bytes as u64
            };

            let mut ctx = context();
            let destination_before = 0xA5A5_5A5A_DEAD_BEEF;
            ctx.write_vreg(k(2), destination_before);
            let exit = execute(
                X86OpmaskOp::MoveToMask {
                    dst: k(2),
                    src: X86OpmaskMoveSource::Memory(Address::Absolute(fault_addr)),
                    width,
                },
                level,
                &mut ctx,
                &mut FlatMemory::new(0x20),
            );
            assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr,
                    write: false,
                }) if addr == reported_addr
            ));
            assert_eq!(ctx.read_vreg(k(2)), destination_before);
            assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_RFLAGS);

            let mut memory = FlatMemory::new(0x20);
            memory.load(0x18, &[0xCC; 8]);
            ctx.write_vreg(k(1), u64::MAX);
            let exit = execute(
                X86OpmaskOp::MoveFromMask {
                    dst: X86OpmaskMoveDestination::Memory(Address::Absolute(fault_addr)),
                    src: k(1),
                    width,
                },
                level,
                &mut ctx,
                &mut memory,
            );
            assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr,
                    write: true,
                }) if addr == reported_addr
            ));
            assert_eq!(read_bytes(&mut memory, 0x18, 8), vec![0xCC; 8]);
            assert_eq!(ctx.read_vreg(k(1)), u64::MAX);
            assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_RFLAGS);
        }
    }
}

#[test]
fn ktest_and_kortest_set_only_cf_zf_and_clear_of_sf_af_pf_for_every_width() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let mask = width.mask();
            for (kind, lhs, rhs, expected_zf, expected_cf) in [
                (X86OpmaskTestKind::And, 0, 0, true, true),
                (X86OpmaskTestKind::And, 0, 1, true, false),
                (X86OpmaskTestKind::And, mask, 1, false, true),
                (X86OpmaskTestKind::Or, 0, 0, true, false),
                (X86OpmaskTestKind::Or, 1, 2, false, false),
                (X86OpmaskTestKind::Or, mask, 0, false, true),
            ] {
                let mut ctx = context();
                ctx.write_vreg(k(1), lhs);
                ctx.write_vreg(k(2), rhs);
                assert_halt(execute(
                    X86OpmaskOp::Test {
                        kind,
                        src1: k(1),
                        src2: k(2),
                        width,
                    },
                    level,
                    &mut ctx,
                    &mut FlatMemory::new(1),
                ));
                assert_eq!(ctx.flags.materialized.zf, expected_zf, "{kind:?} {width:?}");
                assert_eq!(ctx.flags.materialized.cf, expected_cf, "{kind:?} {width:?}");
                assert!(!ctx.flags.materialized.of, "{kind:?} {width:?}");
                assert!(!ctx.flags.materialized.sf, "{kind:?} {width:?}");
                assert!(!ctx.flags.materialized.af, "{kind:?} {width:?}");
                assert!(!ctx.flags.materialized.pf, "{kind:?} {width:?}");
                assert_eq!(ctx.read_vreg(k(1)), lhs);
                assert_eq!(ctx.read_vreg(k(2)), rhs);
            }
        }
    }
}

#[test]
fn opmask_metadata_is_exact_and_generic_jit_admission_remains_fail_closed() {
    let address = Address::BaseIndexScale {
        base: Some(gpr(X86Reg::Rax)),
        index: gpr(X86Reg::Rcx),
        scale: 4,
        disp: 8,
        disp_size: crate::smir::ir::types::DispSize::Disp8,
    };
    let load = X86OpmaskOp::MoveToMask {
        dst: k(2),
        src: X86OpmaskMoveSource::Memory(address.clone()),
        width: OpWidth::W32,
    };
    let load_kind = OpKind::X86Opmask(load.clone());
    assert_eq!(load.dests(), vec![k(2)]);
    assert_eq!(
        load.source_vregs(),
        vec![gpr(X86Reg::Rcx), gpr(X86Reg::Rax)]
    );
    assert_eq!(load.memory_address(), Some(&address));
    assert!(load_kind.reads_memory());
    assert!(!load_kind.writes_memory());
    assert!(load_kind.has_side_effects());
    assert!(!load_kind.is_jit_safe());

    let store = X86OpmaskOp::MoveFromMask {
        dst: X86OpmaskMoveDestination::Memory(address.clone()),
        src: k(3),
        width: OpWidth::W64,
    };
    let store_kind = OpKind::X86Opmask(store.clone());
    assert!(store.dests().is_empty());
    assert_eq!(
        store.source_vregs(),
        vec![k(3), gpr(X86Reg::Rcx), gpr(X86Reg::Rax)]
    );
    assert_eq!(store.memory_address(), Some(&address));
    assert!(!store_kind.reads_memory());
    assert!(store_kind.writes_memory());
    assert!(store_kind.has_side_effects());
    assert!(!store_kind.is_jit_safe());

    let test = OpKind::X86Opmask(X86OpmaskOp::Test {
        kind: X86OpmaskTestKind::And,
        src1: k(1),
        src2: k(2),
        width: OpWidth::W16,
    });
    assert_eq!(test.flags_written(), FlagSet::ALL_X86);
    assert!(
        !test.has_side_effects(),
        "flag liveness, not an unconditional side-effect marker, retains KTEST"
    );
    assert!(
        !test.is_jit_safe(),
        "target gate, not generic gate, admits K ops"
    );
}
