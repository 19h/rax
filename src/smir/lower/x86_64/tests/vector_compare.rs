//! Exact state-backed x86-64 lowering for VPCOM register and memory forms.

use super::*;
use crate::smir::OpId;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, FunctionId, VReg, VecCmpCond, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{FunctionBuilder, SmirFunction, Terminator};
use crate::smir::lower::x86_64::x86_state_vcmp_shape_valid;
use crate::smir::lower::{LowerError, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET};

const PC: u64 = 0x3456;
const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
const STATUS_FLAGS: u64 = 1 | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);
const CONDITIONS: &[VecCmpCond] = &[
    VecCmpCond::Eq,
    VecCmpCond::Ne,
    VecCmpCond::Lt,
    VecCmpCond::Le,
    VecCmpCond::Gt,
    VecCmpCond::Ge,
    VecCmpCond::Ltu,
    VecCmpCond::Leu,
    VecCmpCond::Gtu,
    VecCmpCond::Geu,
    VecCmpCond::False,
    VecCmpCond::True,
];
const SHAPES: &[(VecElementType, u8)] = &[
    (VecElementType::I8, 16),
    (VecElementType::I16, 8),
    (VecElementType::I32, 4),
    (VecElementType::I64, 2),
];
const OPCODES: &[(u8, VecElementType)] = &[
    (0xCC, VecElementType::I8),
    (0xCD, VecElementType::I16),
    (0xCE, VecElementType::I32),
    (0xCF, VecElementType::I64),
    (0xEC, VecElementType::I8),
    (0xED, VecElementType::I16),
    (0xEE, VecElementType::I32),
    (0xEF, VecElementType::I64),
];

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn compare(
    dst: VReg,
    src1: VReg,
    src2: VReg,
    elem: VecElementType,
    lanes: u8,
    cond: VecCmpCond,
) -> OpKind {
    OpKind::VCmp {
        dst,
        src1,
        src2,
        cond,
        elem,
        lanes,
    }
}

fn function_with(ops: Vec<OpKind>) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for kind in ops {
        builder.push_op(PC, kind);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    for op in &mut function.blocks[0].ops {
        if matches!(op.kind, OpKind::VCmp { .. }) {
            op.x86_hint = Some(X86OpHint::XopVpcom);
        }
    }
    function
}

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

fn memory_function(elem: VecElementType, lanes: u8, cond: VecCmpCond) -> SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx)));
    let mut function = function_with(vec![
        OpKind::X86RequireXop,
        OpKind::X86CheckAlignmentAc {
            addr: addr.clone(),
            access_size: 16,
            alignment: 16,
            stack_segment: false,
            natural_alignment: false,
        },
        OpKind::VLoad {
            dst: temporary,
            addr,
            width: VecWidth::V128,
        },
        compare(xmm(1), xmm(2), temporary, elem, lanes, cond),
    ]);
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    function
}

fn lane(words: &[u64; 8], offset: usize, element_bytes: usize) -> u64 {
    let word = offset / 8;
    let byte = offset % 8;
    let mut raw = words[word] >> (byte * 8);
    if byte + element_bytes > 8 {
        raw |= words[word + 1] << ((8 - byte) * 8);
    }
    if element_bytes == 8 {
        raw
    } else {
        raw & ((1_u64 << (element_bytes * 8)) - 1)
    }
}

fn signed(value: u64, bits: u32) -> i64 {
    if bits == 64 {
        value as i64
    } else {
        let shift = 64 - bits;
        ((value << shift) as i64) >> shift
    }
}

fn reference(
    source1: [u64; 8],
    source2: [u64; 8],
    elem: VecElementType,
    cond: VecCmpCond,
) -> [u64; 8] {
    let element_bytes = elem.bytes() as usize;
    let bits = (element_bytes * 8) as u32;
    let mut result = [0_u64; 8];
    for offset in (0..16).step_by(element_bytes) {
        let left = lane(&source1, offset, element_bytes);
        let right = lane(&source2, offset, element_bytes);
        let set = match cond {
            VecCmpCond::Eq => left == right,
            VecCmpCond::Ne => left != right,
            VecCmpCond::Lt => signed(left, bits) < signed(right, bits),
            VecCmpCond::Le => signed(left, bits) <= signed(right, bits),
            VecCmpCond::Gt => signed(left, bits) > signed(right, bits),
            VecCmpCond::Ge => signed(left, bits) >= signed(right, bits),
            VecCmpCond::Ltu => left < right,
            VecCmpCond::Leu => left <= right,
            VecCmpCond::Gtu => left > right,
            VecCmpCond::Geu => left >= right,
            VecCmpCond::False => false,
            VecCmpCond::True => true,
        };
        if set {
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1_u64 << bits) - 1
            };
            result[offset / 8] |= mask << ((offset % 8) * 8);
        }
    }
    result
}

#[test]
fn register_shape_accepts_every_integer_condition_and_alias_but_fails_closed_elsewhere() {
    for &(elem, lanes) in SHAPES {
        for &cond in CONDITIONS {
            for (dst, src1, src2) in [(1, 2, 3), (2, 2, 3), (3, 2, 3), (2, 2, 2), (15, 13, 14)] {
                let kind = compare(xmm(dst), xmm(src1), xmm(src2), elem, lanes, cond);
                let op = SmirOp::with_hint(OpId(0), PC, kind.clone(), X86OpHint::XopVpcom);
                assert!(x86_state_vcmp_shape_valid(&op), "{kind:?}");
                lower(&function_with(vec![kind]), false, false, false, false)
                    .expect("lower exact state-backed VCmp");
            }
        }
    }

    for kind in [
        compare(
            xmm(1),
            xmm(2),
            xmm(3),
            VecElementType::F32,
            4,
            VecCmpCond::Eq,
        ),
        compare(
            xmm(1),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            15,
            VecCmpCond::Eq,
        ),
        compare(
            xmm(16),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            16,
            VecCmpCond::Eq,
        ),
        compare(
            VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            16,
            VecCmpCond::Eq,
        ),
        compare(
            VReg::Virtual(VirtualId(0)),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            16,
            VecCmpCond::Eq,
        ),
    ] {
        let op = SmirOp::with_hint(OpId(0), PC, kind.clone(), X86OpHint::XopVpcom);
        assert!(!x86_state_vcmp_shape_valid(&op), "{kind:?}");
        assert!(matches!(
            lower(&function_with(vec![kind]), false, false, false, false),
            Err(LowerError::InvalidOperand { .. })
                | Err(LowerError::InvalidRegister(_))
                | Err(LowerError::UnsupportedOp { .. })
        ));
    }

    let mut hinted = function_with(vec![compare(
        xmm(1),
        xmm(2),
        xmm(3),
        VecElementType::I8,
        16,
        VecCmpCond::Ne,
    )]);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_state_vcmp_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(matches!(
        lower(&hinted, false, false, false, false),
        Err(LowerError::UnsupportedOp { .. })
            | Err(LowerError::InvalidOperand { .. })
            | Err(LowerError::InvalidRegister(_))
    ));

    let mut unhinted = function_with(vec![compare(
        xmm(1),
        xmm(2),
        xmm(3),
        VecElementType::I8,
        16,
        VecCmpCond::Ne,
    )]);
    unhinted.blocks[0].ops[0].x86_hint = None;
    assert!(!x86_state_vcmp_shape_valid(&unhinted.blocks[0].ops[0]));
    assert!(matches!(
        lower(&unhinted, false, false, false, false),
        Err(LowerError::UnsupportedOp { .. })
            | Err(LowerError::InvalidOperand { .. })
            | Err(LowerError::InvalidRegister(_))
    ));
}

#[test]
fn register_lowering_references_every_slot_and_preserves_flags_explicitly() {
    let function = function_with(vec![compare(
        xmm(1),
        xmm(2),
        xmm(3),
        VecElementType::I64,
        2,
        VecCmpCond::Geu,
    )]);
    let (code, _) = lower(&function, false, false, false, false).expect("lower state-backed VCmp");
    for index in 1..=3 {
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
fn strict_lifted_vpcom_reaches_register_and_memory_native_lowering_for_every_cell() {
    for &(opcode, _) in OPCODES {
        for predicate in 0..8 {
            let register = [0x8F, 0xE8, 0x68, opcode, 0xD9, 0xA0 | predicate, 0xF4];
            lower_jit_guarded_x86_block(&register, false);

            let memory = [0x8F, 0xE8, 0x68, opcode, 0x08, 0xF0 | predicate, 0xF4];
            lower_jit_guarded_x86_block(&memory, true);
        }
    }
}

#[cfg(feature = "smir-jit")]
#[test]
fn memory_pair_requires_helpers_and_vector_preservation_for_physical_state() {
    for &(elem, lanes) in SHAPES {
        for &cond in CONDITIONS {
            let function = memory_function(elem, lanes, cond);
            let (code, _) = lower(&function, true, false, false, false)
                .expect("lower helper-backed VPCOM pair");
            let required_offsets: &[_] = if matches!(cond, VecCmpCond::False | VecCmpCond::True) {
                &[X86_GUEST_ZMM_OFFSET + 64]
            } else {
                &[
                    X86_GUEST_VECTOR_SCRATCH_OFFSET,
                    X86_GUEST_ZMM_OFFSET + 64,
                    X86_GUEST_ZMM_OFFSET + 128,
                ]
            };
            for &offset in required_offsets {
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
                .expect("vector-preserving helper admits physical-vector VPCOM pair");
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
            0x807F_FF00_0123_FEDC_u64.rotate_left((index * 11 + word * 19) as u32)
        });
    }
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_register_compare_matches_reference_for_every_shape_condition_and_alias() {
    use crate::smir::lower::runtime::ExecMem;

    for &(elem, lanes) in SHAPES {
        for &cond in CONDITIONS {
            for (name, dst, src1, src2) in [
                ("distinct", 1, 2, 3),
                ("destination-left", 2, 2, 3),
                ("destination-right", 3, 2, 3),
                ("sources-alias", 1, 2, 2),
                ("all-operands", 2, 2, 2),
            ] {
                let function = function_with(vec![compare(
                    xmm(dst),
                    xmm(src1),
                    xmm(src2),
                    elem,
                    lanes,
                    cond,
                )]);
                let (code, entry) = lower(&function, false, false, false, false)
                    .expect("lower native state-backed VCmp");
                let exec = ExecMem::new(&code).expect("map native state-backed VCmp");
                let mut regs = initialized_regs();
                let expected = reference(
                    regs.zmm[usize::from(src1)],
                    regs.zmm[usize::from(src2)],
                    elem,
                    cond,
                );
                let initial_gpr = regs.gpr;
                let initial_flags = regs.rflags;
                exec.run(entry, &mut regs);
                assert_eq!(
                    regs.zmm[usize::from(dst)],
                    expected,
                    "{name}, {elem:?}, {cond:?}"
                );
                assert_eq!(regs.gpr, initial_gpr, "{name}, {elem:?}, {cond:?}");
                assert_eq!(regs.rflags, initial_flags, "{name}, {elem:?}, {cond:?}");
                assert_eq!(regs.exit_pc, SENTINEL_PC, "{name}, {elem:?}, {cond:?}");
            }
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
    if !context.ok || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX || size != 16
    {
        return 0;
    }
    state.vector_scratch = context.value;
    1
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_memory_pair_matches_reference_and_faults_before_destination_commit() {
    use crate::smir::lower::runtime::ExecMem;

    for &(elem, lanes) in SHAPES {
        for &cond in CONDITIONS {
            let function = memory_function(elem, lanes, cond);
            let (code, entry) =
                lower(&function, true, false, false, false).expect("lower memory VPCOM");
            let exec = ExecMem::new(&code).expect("map memory VPCOM");
            for ok in [true, false] {
                let memory = std::array::from_fn(|word| {
                    0x8000_7FFF_FFFF_0001_u64.rotate_left((word * 13) as u32)
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
                    expected.vector_scratch = memory;
                    expected.zmm[1] = reference(initial.zmm[2], memory, elem, cond);
                } else {
                    expected.exit_pc = PC;
                }
                exec.run(entry, &mut regs);
                expected.host_mxcsr = regs.host_mxcsr;
                assert_eq!(regs, expected, "{elem:?}, {cond:?}, ok={ok}");
                assert_eq!(context.calls, 1);
                assert_eq!(context.address, 0x4000);
                assert_eq!(
                    context.destination,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
                );
                assert_eq!(context.size, 16);
                assert_eq!(context.zero_upper, 1);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn mixed_native_vector_and_state_compare_synchronize_both_directions() {
    use crate::smir::lower::runtime::{ExecMem, X86_VECTOR_STATE_YMM16};

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping mixed VCmp synchronization: host lacks AVX");
        return;
    }

    let mut function = function_with(vec![
        OpKind::VAdd {
            dst: xmm(2),
            src1: xmm(2),
            src2: xmm(5),
            elem: VecElementType::I8,
            lanes: 16,
        },
        compare(
            xmm(1),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            16,
            VecCmpCond::Le,
        ),
        OpKind::VAnd {
            dst: xmm(0),
            src1: xmm(1),
            src2: xmm(6),
            width: VecWidth::V128,
        },
    ]);
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xFC,
        width: VecWidth::V128,
        w: false,
    });
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xDB,
        width: VecWidth::V128,
        w: false,
    });
    let (code, entry) =
        lower(&function, false, false, true, true).expect("lower mixed VCmp region");
    let exec = ExecMem::new(&code).expect("map mixed VCmp region");
    let mut regs = initialized_regs();
    regs.vector_active = X86_VECTOR_STATE_YMM16;
    regs.zmm[6] = [u64::MAX; 8];

    let mut produced_bytes = [0_u8; 16];
    for word in 0..2 {
        let source = regs.zmm[2][word].to_le_bytes();
        let addend = regs.zmm[5][word].to_le_bytes();
        for byte in 0..8 {
            produced_bytes[word * 8 + byte] = source[byte].wrapping_add(addend[byte]);
        }
    }
    let mut produced = regs.zmm[2];
    for word in 0..2 {
        produced[word] =
            u64::from_le_bytes(produced_bytes[word * 8..word * 8 + 8].try_into().unwrap());
    }
    produced[2..].fill(0);
    let expected = reference(produced, regs.zmm[3], VecElementType::I8, VecCmpCond::Le);
    let initial_gpr = regs.gpr;
    let initial_flags = regs.rflags;
    exec.run(entry, &mut regs);
    assert_eq!(
        regs.zmm[2][..2],
        produced[..2],
        "native producer -> state-backed input"
    );
    assert_eq!(regs.zmm[1], expected, "state-backed destination");
    assert_eq!(
        regs.zmm[0][..2],
        expected[..2],
        "state-backed result -> native consumer"
    );
    assert_eq!(regs.gpr, initial_gpr);
    assert_eq!(regs.rflags, initial_flags);
}
