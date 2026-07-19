//! RTM deterministic fallback native-lowering tests.

use super::*;
use crate::smir::lower::x86_64::*;

const XTEST_SEQUENCE: [u8; 15] = [
    0x9C, // pushfq
    0x48, 0x81, 0x24, 0x24, 0x2A, 0xF7, 0xFF, 0xFF, // and qword [rsp], !08D5h
    0x48, 0x83, 0x0C, 0x24, 0x40, // or qword [rsp], 40h
    0x9D, // popfq
];

#[test]
fn lower_xtest_emits_stack_only_exact_flag_transform() {
    let code = lower_single_op(OpKind::X86XTest);
    assert!(
        code.windows(XTEST_SEQUENCE.len())
            .any(|window| window == XTEST_SEQUENCE),
        "missing exact XTEST sequence: {code:02X?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_xtest_preserves_gprs_and_non_status_flags() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const STATUS: u64 = 0x08D5;
    const DF: u64 = 1 << 10;

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::X86XTest);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower XTEST");
    let code = lowerer.finalize().expect("finalize XTEST");
    let exec = ExecMem::new(&code).expect("map XTEST");

    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0x0102_0304_0506_0708u64
            .wrapping_add((index as u64).wrapping_mul(0x1111_1111_1111_1111));
    }
    regs.rflags = 0x2 | STATUS | DF;
    let expected_gprs = regs.gpr;

    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.gpr, expected_gprs);
    assert_eq!(regs.rflags & STATUS, 1 << 6);
    assert_eq!(regs.rflags & DF, DF);
}
