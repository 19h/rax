//! Strict lift coverage for the legacy `CPUID` encoding.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, X86RegState};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;

fn exact_cpuid(result: &LiftResult) -> &SmirOp {
    result
        .ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86Cpuid { .. }))
        .expect("one exact CPUID semantic op")
}

fn cpuid_block() -> SmirBlock {
    let result = lift_single(&[0x0F, 0xA2]).expect("strict CPUID lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_cpuid(
    leaf: u32,
    subleaf: u32,
    configure: impl FnOnce(&mut X86RegState),
) -> ([u64; 4], SmirContext) {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    configure(x86);
    context.write_vreg(x86_gpr(0), 0xFFFF_FFFF_0000_0000 | u64::from(leaf));
    context.write_vreg(x86_gpr(1), 0xEEEE_EEEE_0000_0000 | u64::from(subleaf));
    context.write_vreg(x86_gpr(2), u64::MAX);
    context.write_vreg(x86_gpr(3), u64::MAX);
    assert!(matches!(
        SmirInterpreter::new()
            .execute_block(&mut context, &mut FlatMemory::new(1), &cpuid_block(),),
        BlockResult::Exit(ExitReason::Halt)
    ));
    (
        [
            context.read_vreg(x86_gpr(0)),
            context.read_vreg(x86_gpr(3)),
            context.read_vreg(x86_gpr(1)),
            context.read_vreg(x86_gpr(2)),
        ],
        context,
    )
}

#[test]
fn cpuid_strictly_lifts_without_an_interpreter_frontier() {
    let result = lift_single(&[0x0F, 0xA2]).expect("strict CPUID lift");

    assert_eq!(result.bytes_consumed, 2);
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_eq!(result.ops.len(), 1, "CPUID requires one atomic SMIR op");
    let op = exact_cpuid(&result);
    assert!(matches!(
        op.kind,
        OpKind::X86Cpuid {
            dst_eax: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            dst_ebx: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
            dst_ecx: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            dst_edx: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            leaf: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            subleaf: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
        }
    ));
    assert_eq!(op.kind.source_vregs(), vec![x86_gpr(0), x86_gpr(1)]);
    assert_eq!(
        op.kind.dests(),
        vec![x86_gpr(0), x86_gpr(3), x86_gpr(1), x86_gpr(2)]
    );
    assert!(op.kind.has_side_effects(), "CPUID is serializing");
    assert!(op.is_jit_safe(), "exact CPUID has helper-backed lowering");
}

#[test]
fn cpuid_accepts_ignored_legacy_rex_and_address_prefixes_but_rejects_lock() {
    let bytes = [0x66, 0xF2, 0xF3, 0x67, 0x64, 0x48, 0x0F, 0xA2];
    let result = lift_single(&bytes).expect("CPUID accepts non-LOCK prefixes");
    assert_eq!(result.bytes_consumed, bytes.len());
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(matches!(
        result.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Cpuid { .. },
            ..
        }]
    ));

    for rex2 in [&[0xD5, 0x80, 0xA2][..], &[0xD5, 0xFF, 0xA2][..]] {
        let result = lift_single(rex2).expect("CPUID accepts ignored REX2 fields");
        assert_eq!(result.bytes_consumed, rex2.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        let ops = assert_rex2_guarded_ops(&result, 1);
        assert!(matches!(
            ops,
            [SmirOp {
                kind: OpKind::X86Cpuid { .. },
                ..
            }]
        ));
    }

    // REX2 is the final prefix. With M=0, a following legacy 0F escape is
    // itself the map-0 opcode and is reserved before CPUID can be decoded.
    let explicit_escape = lift_single(&[0xD5, 0x00, 0x0F, 0xA2])
        .expect("REX2 followed by a legacy 0F escape is an explicit #UD");
    assert_invalid_opcode_trap(&explicit_escape, 3);

    for locked in [&[0xF0, 0x0F, 0xA2][..], &[0xF0, 0xD5, 0x80, 0xA2][..]] {
        assert!(matches!(
            lift_single(locked),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    let locked_explicit_escape = lift_single(&[0xF0, 0xD5, 0x00, 0x0F, 0xA2])
        .expect("reserved REX2 opcode precedes LOCK legality");
    assert_invalid_opcode_trap(&locked_explicit_escape, 4);
}

#[test]
fn cpuid_interpreter_returns_exact_vendor_and_zero_extends_all_outputs() {
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
    let (outputs, mut context) = execute_cpuid(0, 0xFFFF_FFFF, |_| {});
    assert_eq!(outputs, [0x29, 0x756E_6547, 0x6C65_746E, 0x4965_6E69]);

    context.flags.materialized = flags;
    context.write_vreg(x86_gpr(0), 0);
    context.write_vreg(x86_gpr(1), 0);
    assert!(matches!(
        SmirInterpreter::new()
            .execute_block(&mut context, &mut FlatMemory::new(1), &cpuid_block(),),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    assert!(context.flags.lazy.is_none());
}

#[test]
fn cpuid_interpreter_tracks_every_mutable_guest_profile_input() {
    let (osxsave_clear, _) = execute_cpuid(1, 0, |_| {});
    let (osxsave_set, _) = execute_cpuid(1, 0, |x86| x86.cr4 = 1 << 18);
    assert_eq!(osxsave_clear[2] & (1 << 27), 0);
    assert_ne!(osxsave_set[2] & (1 << 27), 0);

    let (leaf7, _) = execute_cpuid(7, 0, |x86| {
        x86.cr4 = 1 << 22;
        x86.xeon_phi_avx512 = true;
        x86.vp2intersect = true;
    });
    assert_eq!(leaf7[1] & ((1 << 26) | (1 << 27)), (1 << 26) | (1 << 27));
    assert_ne!(leaf7[1] & (1 << 1), 0, "IA32_TSC_ADJUST must be enumerated");
    assert_ne!(leaf7[2] & (1 << 4), 0, "CR4.PKE must drive OSPKE");
    assert_ne!(leaf7[3] & (1 << 8), 0, "VP2INTERSECT gate");
    assert_eq!(
        leaf7[3] & (1 << 18),
        0,
        "PCONFIG must remain absent until its platform-key semantics exist"
    );

    let (leaf7_subleaf1, _) = execute_cpuid(7, 1, |_| {});
    assert_eq!(
        leaf7_subleaf1[0] & ((1 << 19) | (1 << 27)),
        0,
        "WRMSRNS and MSRLIST must remain absent until their MSR semantics exist"
    );

    let (pconfig, _) = execute_cpuid(0x1B, 0, |_| {});
    assert_eq!(
        pconfig, [0; 4],
        "the disabled profile must expose no PCONFIG target"
    );

    let (sse4a, _) = execute_cpuid(0x8000_0001, 0, |x86| x86.sse4a = true);
    assert_ne!(sse4a[2] & (1 << 6), 0);

    let (xsave_apx, _) = execute_cpuid(0xD, 0, |x86| {
        x86.apx_enabled = true;
        x86.xcr0 = 1 | (1 << 19);
    });
    assert_ne!(xsave_apx[0] & (1 << 19), 0);
    let (apx_leaf, _) = execute_cpuid(0x29, 0, |x86| x86.apx_enabled = true);
    assert_eq!(apx_leaf, [0, 1, 0, 0]);
}

#[test]
fn cpuid_o2_retains_the_serializing_operation_even_when_outputs_are_overwritten() {
    let block = cpuid_block();
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    {
        let entry_id = function.entry;
        let entry = function.get_block_mut(entry_id).unwrap();
        for (index, dst) in [x86_gpr(0), x86_gpr(3), x86_gpr(1), x86_gpr(2)]
            .into_iter()
            .enumerate()
        {
            entry.ops.push(SmirOp::new(
                OpId(index as u16 + 1),
                0x1002,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W32,
                },
            ));
        }
    }
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86Cpuid { .. })),
        "DCE must retain CPUID's serializing side effect"
    );
}
