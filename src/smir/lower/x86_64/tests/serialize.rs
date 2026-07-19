//! Native lowering for complete instruction serialization.

use super::*;
use crate::smir::ir::types::FenceKind;

fn serialize_kind() -> OpKind {
    OpKind::Fence {
        kind: FenceKind::InstructionSerialize,
    }
}

fn lower_serialize() -> (Vec<u8>, usize) {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, serialize_kind());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower SERIALIZE");
    assert!(lowered.relocations.is_empty());
    (
        lowerer.finalize().expect("finalize SERIALIZE"),
        lowered.entry_offset,
    )
}

#[test]
fn lower_serialize_uses_portable_cpuid_barrier_and_balanced_state_saves() {
    let (code, _) = lower_serialize();
    let expected = [
        0x9C, 0x50, 0x53, 0x51, 0x52, 0xB8, 0, 0, 0, 0, 0x0F, 0xA2, 0x5A, 0x59, 0x5B, 0x58, 0x9D,
    ];
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "missing register/flag-preserving CPUID barrier: {code:02X?}"
    );
    assert!(
        !code.windows(3).any(|window| window == [0x0F, 0x01, 0xE8]),
        "lowering must not require host SERIALIZE support"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_serialize_preserves_every_guest_register_and_status_flag() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower_serialize();
    let exec = ExecMem::new(&code).expect("map SERIALIZE lowering");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | (index as u64 * 0x0101_0101);
    }
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    let expected_gprs = regs.gpr;
    let expected_flags = regs.rflags;

    exec.run(entry, &mut regs);

    assert_eq!(regs.gpr, expected_gprs);
    const OBSERVABLE: u64 = 0x08D5 | (1 << 10);
    assert_eq!(regs.rflags & OBSERVABLE, expected_flags & OBSERVABLE);
}
