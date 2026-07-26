//! Memory read-modify-write forms whose architectural flag update was proven
//! dead by the optimizer.
//!
//! The lifter emits `Load; compute(flags=None); Store; replay(flags=All)`. When
//! the replay's flag result is dead, optimization deletes it and leaves a
//! three-operation form. Both shapes fuse into the same helper-backed sequence;
//! the shorter one simply has no post-store replay.

use super::*;
use crate::smir::lower::SmirLowerer;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn virt(id: u32) -> VReg {
    VReg::Virtual(crate::smir::ir::types::VirtualId(id))
}

const PC: u64 = 0x1000;

fn addr() -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 8,
        disp_size: DispSize::Disp8,
    }
}

fn load(dst: VReg) -> OpKind {
    OpKind::Load {
        dst,
        addr: addr(),
        width: MemWidth::B4,
        sign: SignExtend::Zero,
    }
}

fn store(src: VReg) -> OpKind {
    OpKind::Store {
        src,
        addr: addr(),
        width: MemWidth::B4,
    }
}

fn or_op(dst: VReg, src1: VReg, flags: FlagUpdate) -> OpKind {
    OpKind::Or {
        dst,
        src1,
        src2: SrcOperand::Imm(2),
        width: OpWidth::W32,
        flags,
    }
}

fn inc_op(dst: VReg, src: VReg, flags: FlagUpdate) -> OpKind {
    OpKind::Inc {
        dst,
        src,
        width: OpWidth::W32,
        flags,
    }
}

fn lower_sequence(ops: Vec<OpKind>) -> Vec<u8> {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for op in ops {
        builder.push_op(PC, op);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer
        .lower_function(&builder.finish())
        .expect("lower fused memory RMW");
    lowerer.finalize().expect("finalize")
}

/// `or dword [rbx+8], 2` reaches `83 /1 ib` against the scratch accumulator.
const OR_EAX_2: [u8; 3] = [0x83, 0xC8, 0x02];
/// `inc eax` is `FF /0`.
const INC_EAX: [u8; 2] = [0xFF, 0xC0];

fn count(bytes: &[u8], needle: &[u8]) -> usize {
    bytes.windows(needle.len()).filter(|w| *w == needle).count()
}

#[test]
fn flag_dead_memory_alu_rmw_fuses_without_a_post_store_replay() {
    let four = lower_sequence(vec![
        load(virt(0)),
        or_op(virt(1), virt(0), FlagUpdate::None),
        store(virt(1)),
        or_op(virt(2), virt(0), FlagUpdate::All),
    ]);
    assert_eq!(
        count(&four, &OR_EAX_2),
        2,
        "the flag-publishing form computes and replays: {four:02X?}"
    );

    let three = lower_sequence(vec![
        load(virt(0)),
        or_op(virt(1), virt(0), FlagUpdate::None),
        store(virt(1)),
    ]);
    assert_eq!(
        count(&three, &OR_EAX_2),
        1,
        "the flag-dead form must compute exactly once: {three:02X?}"
    );
    assert!(
        three.contains(&0x9C) && three.contains(&0x9D),
        "the compute must still be flag-neutral: {three:02X?}"
    );
    // Both forms keep the same fault-precise helper frame.
    assert!(
        three
            .windows(5)
            .any(|b| b == [0x48, 0x8D, 0x64, 0x24, 0xE0]),
        "must reserve the 32-byte caller frame: {three:02X?}"
    );
}

#[test]
fn flag_dead_memory_unary_rmw_fuses_without_a_post_store_replay() {
    let four = lower_sequence(vec![
        load(virt(0)),
        inc_op(virt(1), virt(0), FlagUpdate::None),
        store(virt(1)),
        inc_op(virt(2), virt(0), FlagUpdate::All),
    ]);
    assert_eq!(
        count(&four, &INC_EAX),
        2,
        "the flag-publishing form computes and replays: {four:02X?}"
    );

    let three = lower_sequence(vec![
        load(virt(0)),
        inc_op(virt(1), virt(0), FlagUpdate::None),
        store(virt(1)),
    ]);
    assert_eq!(
        count(&three, &INC_EAX),
        1,
        "the flag-dead form must compute exactly once: {three:02X?}"
    );
    assert!(
        three.contains(&0x9C) && three.contains(&0x9D),
        "the compute must still be flag-neutral: {three:02X?}"
    );
}
