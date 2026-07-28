//! Exact state-backed x86-64 lowering for VBitSelect and its memory pair.

use super::*;
use crate::smir::OpId;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, FunctionId, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{FunctionBuilder, SmirFunction, Terminator};
use crate::smir::lower::x86_64::x86_vbit_select_shape_valid;
use crate::smir::lower::{LowerError, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET};

const PC: u64 = 0x3456;
const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
const STATUS_FLAGS: u64 = 1 | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V64 | VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
    }))
}

fn select(dst: VReg, mask: VReg, src_true: VReg, src_false: VReg, width: VecWidth) -> OpKind {
    OpKind::VBitSelect {
        dst,
        mask,
        src_true,
        src_false,
        width,
    }
}

fn function_with(ops: Vec<OpKind>) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for kind in ops {
        builder.push_op(PC, kind);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

#[allow(clippy::too_many_arguments)]
fn lower(
    function: &SmirFunction,
    mem_helpers: bool,
    preserve_vectors: bool,
    native_vector_state: bool,
    avx_ymm16_state: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_preserve_vector_mem_helpers(preserve_vectors);
    lowerer.set_native_vector_state_active(native_vector_state);
    lowerer.set_avx_ymm16_vector_state(avx_ymm16_state);
    let lowered = lowerer.lower_function(function)?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

fn memory_function(width: VecWidth, memory_is_mask: bool) -> SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx)));
    let mut function = function_with(vec![
        OpKind::X86RequireXop,
        OpKind::X86CheckAlignmentAc {
            addr: addr.clone(),
            access_size: width.bytes() as u8,
            alignment: 16,
            stack_segment: false,
        },
        OpKind::VLoad {
            dst: temporary,
            addr,
            width,
        },
        if memory_is_mask {
            select(
                vector(1, width),
                temporary,
                vector(2, width),
                vector(3, width),
                width,
            )
        } else {
            select(
                vector(1, width),
                vector(3, width),
                vector(2, width),
                temporary,
                width,
            )
        },
    ]);
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    function
}

fn reference(mask: [u64; 8], src_true: [u64; 8], src_false: [u64; 8], width: VecWidth) -> [u64; 8] {
    let mut result = [0_u64; 8];
    for word in 0..(width.bytes() / 8) as usize {
        result[word] = (src_true[word] & mask[word]) | (src_false[word] & !mask[word]);
    }
    result
}

#[test]
fn register_shape_accepts_both_widths_and_aliases_but_fails_closed_elsewhere() {
    for width in [VecWidth::V128, VecWidth::V256] {
        for (dst, mask, src_true, src_false) in [
            (1, 2, 3, 4),
            (2, 2, 3, 4),
            (3, 2, 3, 4),
            (4, 2, 3, 4),
            (2, 2, 2, 2),
            (15, 12, 13, 14),
        ] {
            let kind = select(
                vector(dst, width),
                vector(mask, width),
                vector(src_true, width),
                vector(src_false, width),
                width,
            );
            let op = SmirOp::new(OpId(0), PC, kind.clone());
            assert!(x86_vbit_select_shape_valid(&op), "{kind:?}");
            lower(&function_with(vec![kind]), false, false, false, false)
                .expect("lower exact VBitSelect");
        }
    }

    for kind in [
        select(
            vector(1, VecWidth::V64),
            vector(2, VecWidth::V64),
            vector(3, VecWidth::V64),
            vector(4, VecWidth::V64),
            VecWidth::V64,
        ),
        select(
            vector(1, VecWidth::V512),
            vector(2, VecWidth::V512),
            vector(3, VecWidth::V512),
            vector(4, VecWidth::V512),
            VecWidth::V512,
        ),
        select(xmm(16), xmm(2), xmm(3), xmm(4), VecWidth::V128),
        select(xmm(1), xmm(2), xmm(3), xmm(4), VecWidth::V256),
        select(
            VReg::Virtual(VirtualId(0)),
            xmm(2),
            xmm(3),
            xmm(4),
            VecWidth::V128,
        ),
    ] {
        let op = SmirOp::new(OpId(0), PC, kind.clone());
        assert!(!x86_vbit_select_shape_valid(&op), "{kind:?}");
        assert!(matches!(
            lower(&function_with(vec![kind]), false, false, false, false),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut hinted = function_with(vec![select(xmm(1), xmm(2), xmm(3), xmm(4), VecWidth::V128)]);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_vbit_select_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(matches!(
        lower(&hinted, false, false, false, false),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[test]
fn register_lowering_references_every_slot_and_preserves_flags_explicitly() {
    let function = function_with(vec![select(
        vector(1, VecWidth::V256),
        vector(2, VecWidth::V256),
        vector(3, VecWidth::V256),
        vector(4, VecWidth::V256),
        VecWidth::V256,
    )]);
    let (code, _) = lower(&function, false, false, false, false).expect("lower V256 VBitSelect");
    for index in 1..=4 {
        let offset = X86_GUEST_ZMM_OFFSET + index * 64;
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing vector slot {index}: {code:02X?}"
        );
    }
    assert!(code.contains(&0x9C), "missing PUSHFQ: {code:02X?}");
    assert!(code.contains(&0x9D), "missing POPFQ: {code:02X?}");
}

#[cfg(feature = "smir-jit")]
#[test]
fn strict_lifted_vpcmov_reaches_register_and_memory_native_lowering_for_every_w_l_cell() {
    for w in [false, true] {
        for l in [false, true] {
            let p1 = (u8::from(w) << 7) | 0x68 | (u8::from(l) << 2);
            let register = [0x8F, 0xE8, p1, 0xA2, 0xD9, 0x40, 0xF4];
            lower_jit_guarded_x86_block(&register, false);

            let memory = [0x8F, 0xE8, p1, 0xA2, 0x08, 0x40, 0xF4];
            lower_jit_guarded_x86_block(&memory, true);
        }
    }
}

#[cfg(feature = "smir-jit")]
#[test]
fn memory_pair_requires_helpers_and_vector_preservation_when_physical_state_is_active() {
    for width in [VecWidth::V128, VecWidth::V256] {
        for memory_is_mask in [false, true] {
            let function = memory_function(width, memory_is_mask);
            let (code, _) = lower(&function, true, false, false, false)
                .expect("lower helper-backed VBitSelect pair");
            for offset in [X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET + 64] {
                assert!(
                    code.windows(4)
                        .any(|window| window == (offset as u32).to_le_bytes()),
                    "missing state offset {offset}: {code:02X?}"
                );
            }
            assert!(matches!(
                lower(&function, false, false, false, false),
                Err(LowerError::InvalidRegister(_))
                    | Err(LowerError::UnsupportedOp { .. })
                    | Err(LowerError::InvalidOperand { .. })
            ));
            assert!(matches!(
                lower(&function, true, false, true, true),
                Err(LowerError::UnsupportedOp { .. })
            ));
            lower(&function, true, true, true, true)
                .expect("vector-preserving helper admits physical-vector pair");
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn initialized_regs() -> crate::smir::lower::runtime::GuestRegs {
    let mut regs = crate::smir::lower::runtime::GuestRegs {
        gpr: std::array::from_fn(|index| 0xA500_0000_0000_0000 | index as u64),
        rflags: 0x2 | STATUS_FLAGS,
        exit_pc: SENTINEL_PC,
        ..crate::smir::lower::runtime::GuestRegs::default()
    };
    for (index, value) in regs.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687_u64.rotate_left((index * 11 + word * 5) as u32)
        });
    }
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_register_bit_select_matches_reference_for_widths_and_all_alias_classes() {
    use crate::smir::lower::runtime::ExecMem;

    for width in [VecWidth::V128, VecWidth::V256] {
        for (name, dst, mask, src_true, src_false) in [
            ("distinct", 1, 2, 3, 4),
            ("destination-mask", 2, 2, 3, 4),
            ("destination-true", 3, 2, 3, 4),
            ("destination-false", 4, 2, 3, 4),
            ("mask-true", 1, 2, 2, 4),
            ("mask-false", 1, 2, 3, 2),
            ("true-false", 1, 2, 3, 3),
            ("all operands", 2, 2, 2, 2),
        ] {
            let function = function_with(vec![select(
                vector(dst, width),
                vector(mask, width),
                vector(src_true, width),
                vector(src_false, width),
                width,
            )]);
            let (code, entry) =
                lower(&function, false, false, false, false).expect("lower native VBitSelect");
            let exec = ExecMem::new(&code).expect("map native VBitSelect");
            let mut regs = initialized_regs();
            let expected = reference(
                regs.zmm[usize::from(mask)],
                regs.zmm[usize::from(src_true)],
                regs.zmm[usize::from(src_false)],
                width,
            );
            let initial_gpr = regs.gpr;
            let initial_flags = regs.rflags;
            exec.run(entry, &mut regs);
            assert_eq!(regs.zmm[usize::from(dst)], expected, "{name}, {width:?}");
            assert_eq!(regs.gpr, initial_gpr, "{name}, {width:?}");
            assert_eq!(regs.rflags, initial_flags, "{name}, {width:?}");
            assert_eq!(regs.exit_pc, SENTINEL_PC, "{name}, {width:?}");
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
    ok: bool,
    calls: u64,
    address: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn vector_load_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.address = addr;
    context.destination = destination;
    context.size = size;
    context.zero_upper = zero_upper;
    if !context.ok
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 16 | 32)
    {
        return 0;
    }
    state.vector_scratch = [0; 8];
    state.vector_scratch[..(size / 8) as usize]
        .copy_from_slice(&context.value[..(size / 8) as usize]);
    1
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_memory_pair_matches_reference_and_faults_before_destination_commit() {
    use crate::smir::lower::runtime::ExecMem;

    for width in [VecWidth::V128, VecWidth::V256] {
        for memory_is_mask in [false, true] {
            let function = memory_function(width, memory_is_mask);
            let (code, entry) =
                lower(&function, true, false, false, false).expect("lower memory VBitSelect");
            let exec = ExecMem::new(&code).expect("map memory VBitSelect");
            for ok in [true, false] {
                let memory = std::array::from_fn(|word| {
                    0x0123_4567_89AB_CDEF_u64.rotate_left((word * 7) as u32)
                });
                let mut context = VectorMemoryContext {
                    value: memory,
                    ok,
                    calls: 0,
                    address: 0,
                    destination: 0,
                    size: 0,
                    zero_upper: 0,
                };
                let mut regs = initialized_regs();
                regs.gpr[3] = 0x4000;
                regs.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                regs.vec_load_fn = vector_load_helper as usize as u64;
                regs.cpuid_xop = 1;
                regs.cr0 = 1;
                regs.cr4 = 1 << 18;
                regs.xcr0 = 0b110;
                regs.cs_l = 1;
                let initial = regs;
                let mut expected = initial;
                if ok {
                    expected.vector_scratch = [0; 8];
                    expected.vector_scratch[..(width.bytes() / 8) as usize]
                        .copy_from_slice(&memory[..(width.bytes() / 8) as usize]);
                    expected.zmm[1] = if memory_is_mask {
                        reference(memory, initial.zmm[2], initial.zmm[3], width)
                    } else {
                        reference(initial.zmm[3], initial.zmm[2], memory, width)
                    };
                } else {
                    expected.exit_pc = PC;
                }
                exec.run(entry, &mut regs);
                expected.host_mxcsr = regs.host_mxcsr;
                assert_eq!(regs, expected, "mask={memory_is_mask}, {width:?}, ok={ok}");
                assert_eq!(context.calls, 1);
                assert_eq!(context.address, 0x4000);
                assert_eq!(
                    context.destination,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
                );
                assert_eq!(context.size, width.bytes());
                assert_eq!(context.zero_upper, 1);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn mixed_native_vector_and_state_backed_bit_select_synchronize_both_directions() {
    use crate::smir::lower::runtime::{ExecMem, X86_VECTOR_STATE_YMM16};

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping mixed VBitSelect synchronization: host lacks AVX");
        return;
    }

    let width = VecWidth::V256;
    let mut function = function_with(vec![
        OpKind::VAdd {
            dst: vector(2, width),
            src1: vector(2, width),
            src2: vector(5, width),
            elem: VecElementType::I8,
            lanes: 32,
        },
        select(
            vector(1, width),
            vector(3, width),
            vector(2, width),
            vector(4, width),
            width,
        ),
        OpKind::VAnd {
            dst: vector(0, width),
            src1: vector(1, width),
            src2: vector(6, width),
            width,
        },
    ]);
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xFC,
        width,
        w: false,
    });
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xDB,
        width,
        w: false,
    });
    let (code, entry) =
        lower(&function, false, false, true, true).expect("lower mixed VBitSelect region");
    let exec = ExecMem::new(&code).expect("map mixed VBitSelect region");
    let mut regs = initialized_regs();
    regs.vector_active = X86_VECTOR_STATE_YMM16;
    regs.zmm[6] = [u64::MAX; 8];
    let mut produced_bytes = [0_u8; 64];
    for word in 0..4 {
        let source = regs.zmm[2][word].to_le_bytes();
        let addend = regs.zmm[5][word].to_le_bytes();
        for byte in 0..8 {
            produced_bytes[word * 8 + byte] = source[byte].wrapping_add(addend[byte]);
        }
    }
    let mut produced = [0_u64; 8];
    for word in 0..4 {
        produced[word] =
            u64::from_le_bytes(produced_bytes[word * 8..word * 8 + 8].try_into().unwrap());
    }
    let expected = reference(regs.zmm[3], produced, regs.zmm[4], width);
    let initial_gpr = regs.gpr;
    let initial_flags = regs.rflags;
    exec.run(entry, &mut regs);
    assert_eq!(
        regs.zmm[2][..4],
        produced[..4],
        "native producer -> state-backed input"
    );
    assert_eq!(regs.zmm[1], expected, "state-backed destination");
    assert_eq!(
        regs.zmm[0][..4],
        expected[..4],
        "state-backed result -> native consumer"
    );
    assert_eq!(regs.gpr, initial_gpr);
    assert_eq!(regs.rflags, initial_flags);
}
