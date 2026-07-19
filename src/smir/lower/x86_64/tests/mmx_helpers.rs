//! MMX lowering at Rust helper-call boundaries.

use super::*;
use crate::smir::lower::X86_GUEST_MM_OFFSET;

fn mm_state_encoding(index: u8, base: PhysReg, store: bool) -> Vec<u8> {
    let mut bytes = vec![0x0F, if store { 0x7F } else { 0x6F }];
    bytes.push(0x80 | ((index & 7) << 3) | base.low3());
    bytes.extend_from_slice(&(X86_GUEST_MM_OFFSET + i32::from(index) * 8).to_le_bytes());
    bytes
}

fn assert_mmx_helper_boundary(bytes: &[u8], name: &str) {
    let store = mm_state_encoding(0, PhysReg::Rax, true);
    let load = mm_state_encoding(0, PhysReg::Rcx, false);
    let store_pos = bytes
        .windows(store.len())
        .position(|window| window == store)
        .expect("MM0 store position");
    let emms_pos = bytes
        .windows(2)
        .position(|window| window == [0x0F, 0x77])
        .expect("EMMS position");
    let call_pos = bytes
        .windows(2)
        .position(|window| window == [0xFF, 0x90])
        .expect("indirect helper call position");
    let first_load_pos = bytes
        .windows(load.len())
        .position(|window| window == load)
        .expect("MM0 reload position");
    assert!(
        store_pos < emms_pos && emms_pos < call_pos && call_pos < first_load_pos,
        "{name} must spill, execute EMMS, call Rust, then reload MMX"
    );
    assert_eq!(
        bytes
            .windows(store.len())
            .filter(|window| *window == store)
            .count(),
        1,
        "{name} must publish MM0 exactly once: {bytes:02X?}"
    );
    assert_eq!(
        bytes
            .windows(load.len())
            .filter(|window| *window == load)
            .count(),
        2,
        "{name} must reload MM0 on success and failure: {bytes:02X?}"
    );
    assert_eq!(
        bytes
            .windows(2)
            .filter(|window| *window == [0x0F, 0x77])
            .count(),
        1,
        "{name} must execute one host-only EMMS before the Rust call: {bytes:02X?}"
    );
}

#[test]
fn mmx_helper_state_uses_exact_full_file_movq_and_host_emms_encodings() {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.emit_helper_mmx_state(PhysReg::Rax, true);
    let stores = lowerer.code.data().to_vec();
    for index in 0..8 {
        let expected = mm_state_encoding(index, PhysReg::Rax, true);
        assert!(
            stores
                .windows(expected.len())
                .any(|window| window == expected),
            "missing MM{index} store {expected:02X?} in {stores:02X?}"
        );
    }
    assert_eq!(&stores[stores.len() - 2..], &[0x0F, 0x77]);

    lowerer.code.clear();
    lowerer.emit_helper_mmx_state(PhysReg::Rcx, false);
    let loads = lowerer.code.data();
    for index in 0..8 {
        let expected = mm_state_encoding(index, PhysReg::Rcx, false);
        assert!(
            loads
                .windows(expected.len())
                .any(|window| window == expected),
            "missing MM{index} load {expected:02X?} in {loads:02X?}"
        );
    }
    assert!(!loads.windows(2).any(|window| window == [0x0F, 0x77]));
}

#[test]
fn mmx_state_wraps_every_rust_helper_boundary_on_both_return_paths() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let address = Address::Direct(rbx);

    let mut scalar = X86_64Lowerer::new();
    scalar.set_preserve_mmx_helpers(true);
    scalar
        .emit_jit_mem_op(
            0x1000,
            true,
            Some(rax),
            None,
            None,
            None,
            None,
            &address,
            MemWidth::B8,
            SignExtend::Zero,
            0,
        )
        .unwrap();
    assert_mmx_helper_boundary(scalar.code.data(), "scalar MMU helper");

    let mut vector = X86_64Lowerer::new();
    vector.set_preserve_mmx_helpers(true);
    vector
        .emit_jit_vector_mem_op(
            0x1000,
            true,
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            &address,
            VecWidth::V128,
            Some(X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x6F,
            }),
        )
        .unwrap();
    assert_mmx_helper_boundary(vector.code.data(), "vector MMU helper");

    let mut pair = X86_64Lowerer::new();
    pair.set_preserve_mmx_helpers(true);
    pair.emit_jit_pair_op(0x1000, true, rax, rbx).unwrap();
    assert_mmx_helper_boundary(pair.code.data(), "paired MMU helper");

    let mut call = X86_64Lowerer::new();
    call.set_preserve_mmx_helpers(true);
    let continuation = BlockId(7);
    call.block_guest_pcs.insert(continuation, 0x2000);
    call.emit_jit_call_op(&CallTarget::GuestAddr(0x1800), continuation)
        .unwrap();
    assert_mmx_helper_boundary(call.code.data(), "interpreter call helper");
}

fn lower_mmx_movq_memory(is_load: bool) -> Vec<u8> {
    let mm7 = VReg::Arch(ArchReg::X86(X86Reg::Mm(7)));
    let address = Address::BaseOffset {
        base: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
        offset: 8,
        disp_size: DispSize::Disp8,
    };
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        if is_load {
            OpKind::VLoad {
                dst: mm7,
                addr: address,
                width: VecWidth::V64,
            }
        } else {
            OpKind::VStore {
                src: mm7,
                addr: address,
                width: VecWidth::V64,
            }
        },
    );
    builder.push_op(
        0x1000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseMov {
        prefix: X86SsePrefix::None,
        opcode: if is_load { 0x6F } else { 0x7F },
    });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_mmx_helpers(true);
    let result = lowerer.lower_function(&function).unwrap();
    assert!(result.relocations.is_empty());
    lowerer.finalize().unwrap()
}

#[test]
fn mmx_movq_memory_uses_scalar_helper_and_fault_safe_stack_staging() {
    let load = lower_mmx_movq_memory(true);
    assert!(
        load.windows(5)
            .any(|window| window == [0x48, 0x89, 0x44, 0x24, 0x10]),
        "load result must stage at the outer host-stack slot: {load:02X?}"
    );
    assert!(
        load.windows(4)
            .any(|window| window == [0x0F, 0x6F, 0x3C, 0x24]),
        "load must commit the staged value to MM7: {load:02X?}"
    );
    assert_eq!(
        load.windows(5)
            .filter(|window| *window == [0x48, 0x8D, 0x64, 0x24, 0x10])
            .count(),
        2,
        "success and fault paths must each release the outer stack slot"
    );
    assert_mmx_helper_boundary(&load, "MMX MOVQ load helper");

    let store = lower_mmx_movq_memory(false);
    assert!(
        store
            .windows(4)
            .any(|window| window == [0x0F, 0x7F, 0x3C, 0x24]),
        "store must stage MM7 before host EMMS: {store:02X?}"
    );
    assert!(
        store
            .windows(5)
            .any(|window| window == [0x48, 0x8B, 0x54, 0x24, 0x10]),
        "store helper must read the staged outer slot: {store:02X?}"
    );
    assert_eq!(
        store
            .windows(5)
            .filter(|window| *window == [0x48, 0x8D, 0x64, 0x24, 0x10])
            .count(),
        2,
        "success and fault paths must each release the outer stack slot"
    );
    assert_mmx_helper_boundary(&store, "MMX MOVQ store helper");
}
