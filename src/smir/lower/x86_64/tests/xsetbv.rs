//! Native XSETBV state and source-boundary tests.

use super::*;
use crate::smir::ir::X86InstructionBytes;

fn isolated_xsetbv_function(pc: u64, source: &[u8]) -> SmirFunction {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let mut builder = FunctionBuilder::new(FunctionId(0), pc);
    builder.push_op(
        pc,
        OpKind::X86XSetBv {
            selector: rcx,
            src_low: rax,
            src_high: rdx,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.x86_instruction_bytes.insert(
        (function.blocks[0].id, pc),
        X86InstructionBytes::new(source).expect("bounded XSETBV source"),
    );
    function
}

#[test]
fn xsetbv_lowerer_requires_exact_source_derived_handoff_boundary() {
    let source = [0x0F, 0x01, 0xD1];

    let mut missing = isolated_xsetbv_function(0x1000, &source);
    missing.x86_instruction_bytes.clear();
    assert!(matches!(
        X86_64Lowerer::new().lower_function(&missing),
        Err(LowerError::UnsupportedOp { op })
            if op.contains("without exact source-derived handoff PC")
    ));

    let mut malformed = isolated_xsetbv_function(0x1000, &source);
    malformed.x86_instruction_bytes.insert(
        (malformed.blocks[0].id, 0x1000),
        X86InstructionBytes::new(&[0x66, 0x0F, 0x01, 0xD1]).unwrap(),
    );
    assert!(matches!(
        X86_64Lowerer::new().lower_function(&malformed),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let mut mismatched_next = isolated_xsetbv_function(0x1000, &source);
    mismatched_next.blocks[0]
        .ops
        .push(crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(1),
            0x1004,
            OpKind::Nop,
        ));
    assert!(matches!(
        X86_64Lowerer::new().lower_function(&mismatched_next),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let maximal = {
        let mut source = [0x67; 15];
        source[12..].copy_from_slice(&[0x0F, 0x01, 0xD1]);
        source
    };
    assert!(
        X86_64Lowerer::new()
            .lower_function(&isolated_xsetbv_function(0x1000, &maximal))
            .is_ok()
    );

    let mut unreachable_suffix = isolated_xsetbv_function(0x1000, &source);
    unreachable_suffix.blocks[0]
        .ops
        .push(crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(1),
            0x1003,
            OpKind::Mov {
                dst: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                src: SrcOperand::Imm(0xDEAD),
                width: OpWidth::W64,
            },
        ));
    unreachable_suffix.blocks[0].terminator = Terminator::Trap {
        kind: crate::smir::ir::TrapKind::InvalidOpcode,
    };
    assert!(
        X86_64Lowerer::new()
            .lower_function(&unreachable_suffix)
            .is_ok(),
        "lowering must stop at XSETBV before an unreachable unsafe suffix"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_xsetbv_validates_state_commits_and_hands_off_precisely() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let function = isolated_xsetbv_function(0x1234_5000, &[0x0F, 0x01, 0xD1]);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower isolated state-backed XSETBV");
    let exec = ExecMem::new(&lowerer.finalize().expect("finalize state-backed XSETBV"))
        .expect("map state-backed XSETBV");

    let flags = 0x2 | 0x8D5;
    let run = |value: u64, cr4: u64, cr0: u64, cpl: u64, apx_enabled: bool, selector: u32| {
        let mut regs = GuestRegs::default();
        regs.cr4 = cr4;
        regs.cr0 = cr0;
        regs.cpl = cpl;
        regs.apx_enabled = u64::from(apx_enabled);
        regs.xcr0 = 3;
        regs.gpr[0] = 0xA5A5_A5A5_0000_0000 | (value as u32 as u64);
        regs.gpr[1] = 0x5A5A_5A5A_0000_0000 | u64::from(selector);
        regs.gpr[2] = 0xC3C3_C3C3_0000_0000 | ((value >> 32) as u32 as u64);
        regs.rflags = flags;
        regs.exit_pc = 0;
        let inputs = (regs.gpr[0], regs.gpr[1], regs.gpr[2]);
        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(
            (regs.gpr[0], regs.gpr[1], regs.gpr[2]),
            inputs,
            "XSETBV must preserve EDX:EAX and ECX"
        );
        assert_eq!(regs.rflags & 0x8D5, flags & 0x8D5);
        regs
    };

    for (name, value, apx_enabled) in [
        ("x87", 1, false),
        ("x87+sse", 3, false),
        ("avx", 7, false),
        ("avx512", 0xE7, false),
        ("pkru", 0x2E7, false),
        ("apx", 0x0008_00E7, true),
    ] {
        let regs = run(value, 1 << 18, 1, 0, apx_enabled, 0);
        assert_eq!(regs.xcr0, value, "{name}: committed XCR0");
        assert_eq!(regs.exit_pc, 0x1234_5003, "{name}: next PC handoff");
    }

    // CPL is ignored outside protected mode.
    let real_mode = run(7, 1 << 18, 0, 3, false, 0);
    assert_eq!(real_mode.xcr0, 7);
    assert_eq!(real_mode.exit_pc, 0x1234_5003);

    for (name, value, cr4, cr0, cpl, apx_enabled, selector) in [
        ("OSXSAVE clear", 7, 0, 1, 0, false, 0),
        ("protected CPL3", 7, 1 << 18, 1, 3, false, 0),
        ("selector one", 7, 1 << 18, 1, 0, false, 1),
        ("x87 disabled", 0, 1 << 18, 1, 0, false, 0),
        ("unsupported bit", 9, 1 << 18, 1, 0, false, 0),
        ("AVX without SSE", 5, 1 << 18, 1, 0, false, 0),
        ("partial AVX512", 0x27, 1 << 18, 1, 0, false, 0),
        ("AVX512 without AVX", 0xE3, 1 << 18, 1, 0, false, 0),
        ("APX disabled", 0x0008_0001, 1 << 18, 1, 0, false, 0),
        ("high unsupported", (1u64 << 63) | 1, 1 << 18, 1, 0, true, 0),
    ] {
        let regs = run(value, cr4, cr0, cpl, apx_enabled, selector);
        assert_eq!(regs.xcr0, 3, "{name}: XCR0 must not commit");
        assert_eq!(regs.exit_pc, 0x1234_5000, "{name}: fault restart PC");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_isolated_xsetbv_handoff_uses_every_scanner_source_length() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    for source in [
        &[0x0F, 0x01, 0xD1][..],
        &[0x67, 0x0F, 0x01, 0xD1],
        &[0x64, 0x0F, 0x01, 0xD1],
        &[0x65, 0x0F, 0x01, 0xD1],
        &[0x48, 0x0F, 0x01, 0xD1],
        &[0x44, 0x0F, 0x01, 0xD1],
        &[0x41, 0x0F, 0x01, 0xD1],
        &[0x4D, 0x0F, 0x01, 0xD1],
    ] {
        let function = isolated_xsetbv_function(0x4000, source);
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&function)
            .unwrap_or_else(|error| panic!("lower {source:02X?}: {error:?}"));
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize XSETBV")).expect("map XSETBV");
        let mut regs = GuestRegs::default();
        regs.cr4 = 1 << 18;
        regs.cr0 = 1;
        regs.xcr0 = 3;
        regs.gpr[0] = 7;
        regs.rflags = 0x2 | 0x8D5;
        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(regs.xcr0, 7, "source {source:02X?}");
        assert_eq!(
            regs.exit_pc,
            0x4000 + source.len() as u64,
            "source-derived handoff {source:02X?}"
        );
    }
}
