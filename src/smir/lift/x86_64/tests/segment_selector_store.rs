//! Strict lift, canonical interpretation, optimizer, and oracle-style coverage
//! for `MOV r/m16/32/64, Sreg` (`8C /r`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{
    X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource, X86SystemSelectorStoreOp,
    X86SystemSelectorTarget,
};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_store(result: &LiftResult) -> &X86SystemSelectorStoreOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SystemSelectorStore(store) => store,
        other => panic!("expected one exact X86SystemSelectorStore op, got {other:?}"),
    }
}

fn selector_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict MOV r/m,Sreg lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn set_distinct_selectors(context: &mut SmirContext) {
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.es_selector = 0x0101;
    x86.cs_selector = 0x0202;
    x86.ss_selector = 0x0303;
    x86.ds_selector = 0x0404;
    x86.fs_selector = 0x0505;
    x86.gs_selector = 0x0606;
}

fn selector_value(selector: X86SystemSelector) -> u64 {
    match selector {
        X86SystemSelector::Es => 0x0101,
        X86SystemSelector::Cs => 0x0202,
        X86SystemSelector::Ss => 0x0303,
        X86SystemSelector::Ds => 0x0404,
        X86SystemSelector::Fs => 0x0505,
        X86SystemSelector::Gs => 0x0606,
        X86SystemSelector::Ldtr | X86SystemSelector::Tr => {
            unreachable!("not an 8C /r segment selector")
        }
    }
}

fn x86_arch(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

#[test]
fn mov_rm_sreg_strictly_lifts_every_selector_and_register_width() {
    let selectors = [
        X86SystemSelector::Es,
        X86SystemSelector::Cs,
        X86SystemSelector::Ss,
        X86SystemSelector::Ds,
        X86SystemSelector::Fs,
        X86SystemSelector::Gs,
    ];
    for (field, selector) in selectors.into_iter().enumerate() {
        for (prefix, width) in [
            (&[0x66][..], OpWidth::W16),
            (&[][..], OpWidth::W32),
            (&[0x48][..], OpWidth::W64),
        ] {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x8C, 0xC0 | ((field as u8) << 3)]);
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("{selector:?} {width:?}: {error:?}"));
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
            assert!(matches!(
                exact_store(&result),
                X86SystemSelectorStoreOp {
                    selector: got_selector,
                    target: X86SystemSelectorTarget::Register { dst, width: got_width },
                    requires_apx: false,
                } if *got_selector == selector
                    && *dst == x86_gpr(0)
                    && *got_width == width
            ));
        }
    }
}

#[test]
fn mov_rm_sreg_rex2_map0_exhaustively_ignores_r_fields_and_extends_only_destination() {
    for payload in 0_u8..=0x7F {
        let bytes = [0xD5, payload, 0x8C, 0xC8]; // MOV r32/64,CS
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("REX2 payload {payload:#04x}: {error:?}"));
        let destination =
            (if payload & 0x10 != 0 { 16 } else { 0 }) | (if payload & 0x01 != 0 { 8 } else { 0 });
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            exact_store(&result),
            X86SystemSelectorStoreOp {
                selector: X86SystemSelector::Cs,
                target: X86SystemSelectorTarget::Register { dst, width },
                requires_apx: true,
            } if *dst == x86_gpr(destination)
                && *width == if payload & 0x08 != 0 {
                    OpWidth::W64
                } else {
                    OpWidth::W32
                }
        ));
    }

    let map1 = lift_single(&[0xD5, 0x80, 0x8C, 0, 0, 0, 0])
        .expect("REX2 compressed map 1 row 8 is an explicit #UD");
    assert_invalid_opcode_trap(&map1, 3);
}

#[test]
fn mov_rm_sreg_lifts_exact_memory_shapes_and_fixed_two_byte_store() {
    let absolute = lift_single(&[0x48, 0x8C, 0x0C, 0x25, 0x00, 0x20, 0x00, 0x00])
        .expect("absolute MOV [m16],CS");
    assert!(matches!(
        exact_store(&absolute),
        X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Cs,
            target: X86SystemSelectorTarget::Memory {
                addr: Address::Absolute(0x2000),
            },
            requires_apx: false,
        }
    ));

    let rip_relative =
        lift_single(&[0x8C, 0x15, 0x34, 0x12, 0, 0]).expect("RIP-relative MOV [m16],SS");
    assert!(matches!(
        &exact_store(&rip_relative).target,
        X86SystemSelectorTarget::Memory {
            addr: Address::PcRel {
                offset: 0x1234,
                disp_size: DispSize::Disp32,
                base: Some(0x1006),
            },
        }
    ));

    let fs_sib =
        lift_single(&[0x64, 0x48, 0x8C, 0x64, 0x88, 0x7F]).expect("FS-relative SIB MOV [m16],FS");
    assert!(matches!(
        &exact_store(&fs_sib).target,
        X86SystemSelectorTarget::Memory {
            addr: Address::SegmentRel {
                segment,
                base: Some(base),
                index: Some(index),
                scale: 4,
                disp: 0x7F,
            },
        } if *segment == x86_arch(X86Reg::FsBase)
            && *base == x86_gpr(0)
            && *index == x86_gpr(1)
    ));

    let addr32 = lift_single(&[0x67, 0x8C, 0xAC, 0x8D, 0x78, 0x56, 0x34, 0x12])
        .expect("address-size-32 MOV [m16],GS");
    assert!(matches!(
        &exact_store(&addr32).target,
        X86SystemSelectorTarget::Memory {
            addr: Address::X86Addr32(inner),
        } if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            } if *base == x86_gpr(5) && *index == x86_gpr(1)
        )
    ));

    let apx = lift_single(&[0xD5, 0x33, 0x8C, 0x4C, 0xD1, 0x20])
        .expect("REX2 EGPR base/index MOV [m16],CS");
    assert!(matches!(
        exact_store(&apx),
        X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Cs,
            target: X86SystemSelectorTarget::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    disp: 0x20,
                    ..
                },
            },
            requires_apx: true,
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn mov_rm_sreg_accepts_ignored_prefixes_and_rejects_decode_invalid_forms() {
    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67, 0xF2, 0xF3, 0x40] {
        let bytes = [prefix, 0x8C, 0xC8];
        let result = lift_single(&bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(exact_store(&result).selector, X86SystemSelector::Cs);
    }

    // Legacy REX.R is ignored because ModR/M.reg addresses a segment register.
    let rex_r = lift_single(&[0x4C, 0x8C, 0xC8]).expect("REX.R MOV RAX,CS");
    assert!(matches!(
        exact_store(&rex_r),
        X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Cs,
            target: X86SystemSelectorTarget::Register {
                dst,
                width: OpWidth::W64,
            },
            ..
        } if *dst == x86_gpr(0)
    ));

    for bytes in [
        &[0xF0, 0x8C, 0xC0][..],
        &[0xF0, 0xD5, 0x00, 0x8C, 0xC0],
        &[0x48, 0xD5, 0x00, 0x8C, 0xC0],
        &[0x8C, 0xF0],
        &[0x8C, 0x38],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
    for bytes in [&[0x8C][..], &[0xD5][..], &[0xD5, 0x00], &[0x8C, 0x04]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Incomplete { .. })
        ));
    }
}

#[test]
fn mov_rm_sreg_non_strict_oracle_path_preserves_selector_width_and_length() {
    for (bytes, selector, width) in [
        (&[0x8C, 0xC8][..], X86SystemSelector::Cs, OpWidth::W32),
        (&[0x66, 0x8C, 0xE8], X86SystemSelector::Gs, OpWidth::W16),
        (&[0x48, 0x8C, 0xE0], X86SystemSelector::Fs, OpWidth::W64),
    ] {
        let mut lifter = X86_64Lifter::new();
        let mut context = LiftContext::new(SourceArch::X86_64);
        let result = lifter
            .lift_insn(0x1000, bytes, &mut context)
            .expect("non-strict MOV r/m,Sreg lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            exact_store(&result),
            X86SystemSelectorStoreOp {
                selector: got_selector,
                target: X86SystemSelectorTarget::Register { width: got_width, .. },
                ..
            } if *got_selector == selector && *got_width == width
        ));
    }
}

#[test]
fn mov_rm_sreg_interpreter_reads_all_selectors_commits_exact_widths_and_ignores_system_guards() {
    let incoming = 0xA5A5_5A5A_DEAD_BEEF;
    for (field, selector) in [
        X86SystemSelector::Es,
        X86SystemSelector::Cs,
        X86SystemSelector::Ss,
        X86SystemSelector::Ds,
        X86SystemSelector::Fs,
        X86SystemSelector::Gs,
    ]
    .into_iter()
    .enumerate()
    {
        for (prefix, width) in [
            (&[0x66][..], OpWidth::W16),
            (&[][..], OpWidth::W32),
            (&[0x48][..], OpWidth::W64),
        ] {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x8C, 0xC0 | ((field as u8) << 3)]);
            let mut context = SmirContext::new_x86_64();
            set_distinct_selectors(&mut context);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            // These guards apply to SLDT/STR, never to MOV r/m,Sreg.
            x86.cr0 = 0;
            x86.rflags = crate::isa::x86_64::flags::bits::VM;
            x86.cr4 = 1 << 11;
            x86.cpl = 3;
            context.write_vreg(x86_gpr(0), incoming);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut FlatMemory::new(1),
                &selector_block(&bytes),
            );
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            let value = selector_value(selector);
            let expected = match width {
                OpWidth::W16 => (incoming & !0xFFFF) | value,
                OpWidth::W32 | OpWidth::W64 => value,
                _ => unreachable!(),
            };
            assert_eq!(context.read_vreg(x86_gpr(0)), expected, "{bytes:02X?}");
        }
    }
}

#[test]
fn mov_rm_sreg_interpreter_apx_gate_and_non_x86_shape_are_noncommitting() {
    let block = selector_block(&[0xD5, 0x19, 0x8C, 0xCF]); // MOV R31,CS
    let sentinel = 0xDEAD_BEEF_CAFE_BABE;
    let mut context = SmirContext::new_x86_64();
    set_distinct_selectors(&mut context);
    context.write_vreg(x86_gpr(31), sentinel);

    let disabled =
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
    assert!(matches!(
        disabled,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    assert_eq!(context.read_vreg(x86_gpr(31)), sentinel);

    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.apx_enabled = true;
    let enabled =
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
    assert!(matches!(enabled, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86_gpr(31)), 0x0202);

    let mut arm_context = SmirContext::new_aarch64();
    let non_x86 = SmirInterpreter::new().execute_block(
        &mut arm_context,
        &mut FlatMemory::new(1),
        &selector_block(&[0x8C, 0xC8]),
    );
    assert!(matches!(
        non_x86,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
}

#[test]
fn cs_selector_is_fail_closed_when_injected_into_selector_load_ir() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
            selector: X86SystemSelector::Cs,
            source: X86SystemSelectorSource::Register { src: x86_gpr(0) },
            requires_apx: false,
            next_pc: 0x1003,
        }),
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86_gpr(0), 0x1234);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    assert_eq!(context.read_vreg(x86_gpr(0)), 0x1234);
}

#[test]
fn mov_rm_sreg_interpreter_memory_store_is_exactly_two_bytes_and_fault_precise() {
    let block = selector_block(&[0x8C, 0x08]); // MOV [RAX],CS
    let mut context = SmirContext::new_x86_64();
    set_distinct_selectors(&mut context);
    context.write_vreg(x86_gpr(0), 0x2001);
    let mut memory = FlatMemory::with_base(0x2000, 4);
    memory.load(0, &[0xA5; 4]);
    let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let mut observed = [0; 4];
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0xA5, 0x02, 0x02, 0xA5]);

    context.write_vreg(x86_gpr(0), 0x2003);
    memory.load(0, &[0x5A; 4]);
    let fault = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0x2005,
            write: true,
        })
    ));
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0x5A; 4]);
}

#[test]
fn mov_rm_sreg_optimizer_preserves_every_payload_at_all_levels() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (bytes, selector, width, requires_apx) in [
            (
                &[0x66, 0x8C, 0xC0][..],
                X86SystemSelector::Es,
                OpWidth::W16,
                false,
            ),
            (
                &[0x8C, 0xD8][..],
                X86SystemSelector::Ds,
                OpWidth::W32,
                false,
            ),
            (
                &[0x48, 0x8C, 0xE8][..],
                X86SystemSelector::Gs,
                OpWidth::W64,
                false,
            ),
            (
                &[0xD5, 0x19, 0x8C, 0xCF][..],
                X86SystemSelector::Cs,
                OpWidth::W64,
                true,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, lifted.ops[0].kind.clone());
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            optimize_function(&mut function, level);
            assert!(matches!(
                function.blocks[0].ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
                        selector: got_selector,
                        target: X86SystemSelectorTarget::Register { width: got_width, .. },
                        requires_apx: got_apx,
                    }),
                    ..
                }] if *got_selector == selector
                    && *got_width == width
                    && *got_apx == requires_apx
            ));
        }
    }
}
