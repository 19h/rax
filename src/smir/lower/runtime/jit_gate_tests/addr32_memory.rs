//! Native-admission tests for explicit x86 long-mode addr32 memory operands.

use super::*;
use crate::smir::ir::{SmirBlock, SmirFunction};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};

const PC: u64 = 0x1000;

fn lift_function(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .expect("lift addr32 instruction");
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn scalar_load(addr: Address) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(
        PC,
        OpKind::Load {
            dst: x86(X86Reg::Rax),
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    builder.set_terminator(Terminator::Return { values: Vec::new() });
    builder.finish()
}

#[test]
fn lifted_addr32_scalar_memory_survives_optimization_and_enters_helper_jit() {
    // MOV RAX,[EBX+ECX*4+20h]
    let function = lift_function(&[0x67, 0x48, 0x8B, 0x44, 0x8B, 0x20]);
    let [op] = function.blocks[0].ops.as_slice() else {
        panic!("addr32 MOV must lift without address-materialization operations")
    };
    let OpKind::Load { addr, .. } = &op.kind else {
        panic!("addr32 MOV must lift to Load, got {:?}", op.kind)
    };
    assert!(matches!(
        addr,
        Address::X86Addr32(inner)
            if matches!(
                inner.as_ref(),
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    ..
                } if *base == x86(X86Reg::Rbx) && *index == x86(X86Reg::Rcx)
            )
    ));
    assert_eq!(
        addr.regs(),
        vec![x86(X86Reg::Rcx), x86(X86Reg::Rbx)],
        "the explicit width must not hide architectural address dependencies"
    );
    assert!(x86_jit_scalar_mem_shape_valid(&op.kind));
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
    assert!(!is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        false,
    ));

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O1,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let mut optimized = function.clone();
        crate::smir::optimize::optimize_function(&mut optimized, level);
        let OpKind::Load { addr, .. } = &optimized.blocks[0].ops[0].kind else {
            panic!("optimized addr32 MOV must remain a Load")
        };
        assert!(matches!(addr, Address::X86Addr32(_)));
        assert!(is_native_clobber_safe_excluding(
            &optimized,
            &std::collections::HashMap::new(),
            true,
        ));
    }
}

#[test]
fn explicit_addr32_scalar_memory_gate_accepts_exact_shapes_and_fails_closed() {
    let valid = [
        Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::R31)))),
        Address::X86Addr32(Box::new(Address::BaseIndexScale {
            base: Some(x86(X86Reg::R31)),
            index: x86(X86Reg::R16),
            scale: 8,
            disp: -1,
            disp_size: DispSize::Disp8,
        })),
        Address::X86Addr32(Box::new(Address::Absolute(0x1_0000_0080))),
        Address::X86Addr32(Box::new(Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::Rbx)),
            index: Some(x86(X86Reg::Rcx)),
            scale: 4,
            disp: 0x20,
        })),
    ];
    for addr in valid {
        assert!(x86_jit_mem_address_shape_valid(&addr), "{addr:?}");
        let function = scalar_load(addr);
        assert!(x86_jit_scalar_mem_shape_valid(
            &function.blocks[0].ops[0].kind
        ));
        assert!(is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    let malformed = [
        Address::X86Addr32(Box::new(Address::Direct(VReg::Virtual(VirtualId(1))))),
        Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
            x86(X86Reg::Rax),
        ))))),
        Address::X86Addr32(Box::new(Address::PcRel {
            offset: 4,
            disp_size: DispSize::Disp32,
            base: Some(PC),
        })),
        Address::X86Addr32(Box::new(Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rbx)),
            index: x86(X86Reg::Rcx),
            scale: 3,
            disp: 0,
            disp_size: DispSize::Auto,
        })),
        Address::X86Addr32(Box::new(Address::SegmentRel {
            segment: x86(X86Reg::Rax),
            base: Some(x86(X86Reg::Rbx)),
            index: None,
            scale: 1,
            disp: 0,
        })),
        Address::X86Addr32(Box::new(Address::GpRel { offset: 0 })),
    ];
    for addr in malformed {
        assert!(!x86_jit_mem_address_shape_valid(&addr), "{addr:?}");
        let function = scalar_load(addr);
        assert!(!x86_jit_scalar_mem_shape_valid(
            &function.blocks[0].ops[0].kind
        ));
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    let mut vector_width = scalar_load(Address::X86Addr32(Box::new(Address::Absolute(0))));
    let OpKind::Load { width, .. } = &mut vector_width.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = MemWidth::B16;
    assert!(!x86_jit_scalar_mem_shape_valid(
        &vector_width.blocks[0].ops[0].kind
    ));

    let mut foreign_dst = scalar_load(Address::X86Addr32(Box::new(Address::Absolute(0))));
    let OpKind::Load { dst, .. } = &mut foreign_dst.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *dst = VReg::Arch(ArchReg::Arm(ArmReg::X(0)));
    assert!(!x86_jit_scalar_mem_shape_valid(
        &foreign_dst.blocks[0].ops[0].kind
    ));
}
