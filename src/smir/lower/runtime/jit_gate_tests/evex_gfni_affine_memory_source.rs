//! Helper-backed EVEX affine GFNI memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SrcOperand, VReg, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexGfniAffineMemoryReplay, X86InstructionBytes,
    X86VexGfniMemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_evex_gfni_affine_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x71C0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

fn scanner_encoding(
    inverse: bool,
    ll: u8,
    source1: u8,
    broadcast: bool,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> Vec<u8> {
    assert!(ll < 3 && source1 < 16);
    assert!(mask < 8 && (!zeroing || mask != 0));
    vec![
        0x62,
        0xF3,
        0x85 | (((!source1) & 0x0F) << 3),
        (u8::from(zeroing) << 7) | (ll << 5) | (u8::from(broadcast) << 4) | 0x08 | mask,
        if inverse { 0xCF } else { 0xCE },
        0x02,
        immediate,
    ]
}

fn lift_bytes(bytes: &[u8], level: OptLevel) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("x86 instruction provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| op.kind.reads_memory())
        .expect("affine GFNI memory source")
}

fn virtual_counts(function: &SmirFunction) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &function.blocks[0].ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *definitions.entry(register).or_insert(0) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *uses.entry(register).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence(
    function: &SmirFunction,
) -> crate::smir::lower::runtime::X86JitEvexGfniAffineMemorySequence {
    let index = sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_gfni_affine_memory_sequence(
        &function.blocks[0],
        index,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
    .expect("exact EVEX affine GFNI memory sequence")
}

fn lower(function: &SmirFunction) -> Vec<u8> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
        .lower_function(function)
        .expect("lower EVEX affine GFNI memory source");
    lowerer
        .finalize()
        .expect("finalize EVEX affine GFNI memory source")
}

#[test]
fn representative_full_broadcast_mask_and_optimization_profiles_admit() {
    let cases = [
        (false, 0, 0, false, 0, false, 0x00),
        (false, 1, 1, false, 1, false, 0x63),
        (false, 2, 15, true, 2, true, 0xFF),
        (true, 0, 15, true, 1, false, 0x00),
        (true, 1, 0, false, 2, true, 0x63),
        (true, 2, 1, true, 0, false, 0xFF),
    ];
    let mut admitted = 0usize;
    for (inverse, ll, source1, broadcast, mask, zeroing, immediate) in cases {
        let bytes = scanner_encoding(inverse, ll, source1, broadcast, mask, zeroing, immediate);
        for level in LEVELS {
            let function = lift_bytes(&bytes, level);
            let sequence = sequence(&function);
            assert_eq!(
                sequence.encoding.kind,
                if inverse {
                    X86VexGfniMemoryKind::AffineInverse
                } else {
                    X86VexGfniMemoryKind::Affine
                },
                "{bytes:02X?} {level:?}"
            );
            assert_eq!(
                sequence.encoding.width,
                [VecWidth::V128, VecWidth::V256, VecWidth::V512][usize::from(ll)],
                "{bytes:02X?} {level:?}"
            );
            assert_eq!(sequence.encoding.source1, source1, "{bytes:02X?}");
            assert_eq!(sequence.encoding.immediate, immediate, "{bytes:02X?}");
            assert_eq!(
                sequence.memory_size,
                if broadcast {
                    8
                } else {
                    sequence.encoding.width.bytes()
                },
                "{bytes:02X?}"
            );
            assert!(sequence.consumed > 100, "{bytes:02X?} {level:?}");

            let excluded = HashMap::new();
            assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
            assert!(!is_native_clobber_safe_excluding(
                &function, &excluded, false
            ));
            assert!(!is_x86_aarch64_native_clobber_safe_excluding(
                &function, &excluded
            ));
            assert!(uses_x86_native_vectors_excluding(&function, &excluded));
            assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
                &function, &excluded
            ));

            let requirements = x86_native_replay_feature_requirements(&function, &excluded);
            assert!(requirements.any);
            assert!(requirements.needs_avx);
            assert!(requirements.needs_avx512bw);
            assert_eq!(requirements.needs_avx512vl, ll != 2);
            assert!(requirements.needs_gfni);
            assert!(!requirements.all_spans_support_avx_ymm16);

            let code = lower(&function);
            let emitted = match sequence.encoding.replay {
                X86EvexGfniAffineMemoryReplay::Vector {
                    register_instruction,
                    ..
                } => register_instruction,
                X86EvexGfniAffineMemoryReplay::Broadcast { stack_instruction } => stack_instruction,
            };
            assert!(
                code.windows(emitted.as_slice().len())
                    .any(|window| window == emitted.as_slice()),
                "{bytes:02X?} {level:?}: missing {:02X?}",
                emitted.as_slice()
            );
            admitted += 1;
        }
    }
    assert_eq!(admitted, cases.len() * LEVELS.len());
}

#[test]
fn scanner_universe_admits_and_lowers_all_108_affine_memory_cells() {
    let mut admitted = 0usize;
    let mut lowered = 0usize;
    for inverse in [false, true] {
        for source1 in [0u8, 1, 15] {
            for ll in 0u8..=2 {
                for broadcast in [false, true] {
                    for (mask, zeroing) in [(0u8, false), (1, false), (1, true)] {
                        let bytes =
                            scanner_encoding(inverse, ll, source1, broadcast, mask, zeroing, 0xA5);
                        let function = lift_bytes(&bytes, OptLevel::O2);
                        let sequence = sequence(&function);
                        assert!(is_native_clobber_safe_excluding(
                            &function,
                            &HashMap::new(),
                            true
                        ));
                        admitted += 1;

                        let code = lower(&function);
                        let emitted = match sequence.encoding.replay {
                            X86EvexGfniAffineMemoryReplay::Vector {
                                register_instruction,
                                ..
                            } => register_instruction,
                            X86EvexGfniAffineMemoryReplay::Broadcast { stack_instruction } => {
                                stack_instruction
                            }
                        };
                        assert!(
                            code.windows(emitted.as_slice().len())
                                .any(|window| window == emitted.as_slice()),
                            "{bytes:02X?}: missing {:02X?}",
                            emitted.as_slice()
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(admitted, 108);
    assert_eq!(lowered, 108);
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let index = sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_evex_gfni_affine_memory_sequence(
            &function.blocks[0],
            index,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "{name}: exact matcher admitted malformed sequence"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native gate admitted malformed sequence"
    );
}

#[test]
fn memory_sequence_fails_closed_for_provenance_fault_profile_dataflow_and_boundary_changes() {
    let bytes = scanner_encoding(true, 2, 15, true, 1, false, 0xA5);
    let base = lift_bytes(&bytes, OptLevel::O0);
    let index = sequence_index(&base);

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();

    let mut wrong_provenance = base.clone();
    let full_vector = scanner_encoding(true, 2, 15, false, 1, false, 0xA5);
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&full_vector).unwrap(),
    );

    let mut source_hint = base.clone();
    source_hint.blocks[0].ops[index].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));

    let mut predicated_source = base.clone();
    let (dst, addr, width, sign) = match predicated_source.blocks[0].ops[index].kind.clone() {
        OpKind::Load {
            dst,
            addr,
            width,
            sign,
        } => (dst, addr, width, sign),
        _ => unreachable!("broadcast starts with scalar load"),
    };
    predicated_source.blocks[0].ops[index].kind = OpKind::PredLoad {
        dst,
        cond: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
        addr,
        width,
        signed: sign,
    };

    let mut virtual_address = base.clone();
    match &mut virtual_address.blocks[0].ops[index].kind {
        OpKind::Load { addr, .. } => {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
        _ => unreachable!("broadcast starts with scalar load"),
    }

    let mut wrong_broadcast = base.clone();
    let broadcast = wrong_broadcast.blocks[0]
        .ops
        .iter_mut()
        .skip(index + 1)
        .find(|op| matches!(op.kind, OpKind::VBroadcast { .. }))
        .expect("source broadcast");
    match &mut broadcast.kind {
        OpKind::VBroadcast { elem, .. } => *elem = crate::smir::ir::types::VecElementType::I8,
        _ => unreachable!(),
    }

    let mut child_hint = base.clone();
    child_hint.blocks[0].ops[index + 2].x86_hint =
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));

    let mut child_pc = base.clone();
    child_pc.blocks[0].ops[index + 2].guest_pc += 1;

    let mut wrong_core = base.clone();
    let core = wrong_core.blocks[0]
        .ops
        .iter_mut()
        .skip(index + 2)
        .find(|op| matches!(op.kind, OpKind::VByteShuffle { .. }))
        .expect("affine byte shuffle");
    core.kind = OpKind::Nop;

    let mut wrong_constant = base.clone();
    let constant = wrong_constant.blocks[0]
        .ops
        .iter_mut()
        .skip(index + 2)
        .find(|op| matches!(op.kind, OpKind::Mov { .. }))
        .expect("GFNI splat constant");
    match &mut constant.kind {
        OpKind::Mov {
            src: SrcOperand::Imm(value),
            ..
        } => *value = 0x2A,
        _ => unreachable!(),
    }

    let mut wrong_shift = base.clone();
    let shift = wrong_shift.blocks[0]
        .ops
        .iter_mut()
        .skip(index + 2)
        .find(|op| {
            matches!(
                op.kind,
                OpKind::VShift {
                    amount: SrcOperand::Imm(4),
                    ..
                }
            )
        })
        .expect("GFNI affine parity shift");
    match &mut shift.kind {
        OpKind::VShift {
            amount: SrcOperand::Imm(amount),
            ..
        } => *amount = 3,
        _ => unreachable!(),
    }

    let mut wrong_matrix = base.clone();
    let shuffle = wrong_matrix.blocks[0]
        .ops
        .iter_mut()
        .skip(index + 2)
        .find(|op| matches!(op.kind, OpKind::VByteShuffle { .. }))
        .expect("GFNI affine byte shuffle");
    match &mut shuffle.kind {
        OpKind::VByteShuffle { src, .. } => {
            *src = VReg::Arch(ArchReg::X86(X86Reg::Zmm(15)));
        }
        _ => unreachable!(),
    }

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7F00), PC, OpKind::Nop));

    let external = base.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .find(|reg| matches!(reg, VReg::Virtual(_)))
        .expect("affine graph virtual");
    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F01),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: SrcOperand::Reg(external),
            width: OpWidth::W64,
        },
    ));

    for (name, function) in [
        ("missing provenance", missing_provenance),
        ("provenance memory shape differs", wrong_provenance),
        ("source hint differs", source_hint),
        ("Type E4NF source became predicated", predicated_source),
        ("address contains a virtual register", virtual_address),
        ("broadcast element differs", wrong_broadcast),
        ("semantic child has a hint", child_hint),
        ("semantic child PC differs", child_pc),
        ("affine expansion differs", wrong_core),
        ("GFNI splat constant differs", wrong_constant),
        ("GFNI shift amount differs", wrong_shift),
        ("GFNI matrix dataflow differs", wrong_matrix),
        ("same-PC operation follows sequence", same_pc_tail),
        ("temporary has an external use", external_use),
    ] {
        assert_rejected(name, &function);
    }

    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_gfni_affine_memory_sequence(
            &base.blocks[0],
            index,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );
}

#[test]
fn evex_affine_gfni_memory_rejects_the_avx_only_vector_bridge() {
    let bytes = scanner_encoding(false, 1, 1, false, 1, false, 0x63);
    let function = lift_bytes(&bytes, OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX GFNI");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}

#[test]
fn rip_addr32_segments_and_apx_b4_x4_remain_helper_address_controls() {
    let vector = scanner_encoding(false, 1, 1, false, 1, false, 0x63);
    let broadcast = scanner_encoding(true, 2, 15, true, 1, false, 0xA5);

    let mut rip = vector.clone();
    rip[5] = 0x05;
    rip.splice(6..6, 0x20i32.to_le_bytes());
    let mut addr32 = vector.clone();
    addr32.insert(0, 0x67);
    let mut fs = broadcast.clone();
    fs.insert(0, 0x64);
    let mut gs_addr32 = broadcast.clone();
    gs_addr32[5] = 0x44;
    gs_addr32.splice(6..6, [0x8B, 0x02]);
    gs_addr32.insert(0, 0x67);
    gs_addr32.insert(0, 0x65);

    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let address_cases = [
        (
            "RIP+disp32",
            rip.clone(),
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + rip.len() as u64),
            },
        ),
        (
            "addr32 base",
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rdx)))),
        ),
        (
            "FS broadcast",
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rdx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
        (
            "GS addr32 SIB broadcast",
            gs_addr32,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 16,
            })),
        ),
    ];
    for (name, bytes, expected_address) in address_cases {
        for level in LEVELS {
            let function = lift_bytes(&bytes, level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => {
                        addr == &expected_address
                    }
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function);
            lower(&function);
        }
    }

    let mut apx = vector;
    apx[5] = 0x04;
    apx.insert(6, 0x48); // [RAX + RCX*2]
    apx[1] |= 0x08; // EVEX.B4 extends base to R16
    apx[2] &= !0x04; // EVEX.X4/!U extends index to R17
    let expected_address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::R16)),
        index: x86(X86Reg::R17),
        scale: 2,
        disp: 0,
        disp_size: DispSize::Auto,
    };
    for level in LEVELS {
        let function = lift_bytes(&apx, level);
        assert!(
            matches!(
                function.blocks[0].ops.first().map(|op| &op.kind),
                Some(OpKind::X86RequireApx)
            ),
            "{level:?} {apx:02X?}: APX address lost its dynamic guard"
        );
        assert!(
            function.blocks[0].ops.iter().any(|op| match &op.kind {
                OpKind::VLoad { addr, .. } => addr == &expected_address,
                _ => false,
            }),
            "{level:?} {apx:02X?}: {:#?}",
            function.blocks[0].ops
        );
        sequence(&function);
        lower(&function);
    }
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct GfniMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
    last_signed: u64,
}

#[cfg(target_arch = "x86_64")]
impl GfniMemoryContext {
    fn new(value: [u64; 8], ok: bool) -> Self {
        Self {
            value,
            ok: u64::from(ok),
            calls: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            last_zero_upper: 0,
            last_signed: 0,
        }
    }
}

#[cfg(target_arch = "x86_64")]
extern "C" fn scalar_load_helper(
    context: *mut GfniMemoryContext,
    addr: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = addr;
    context.last_size = size as u32;
    context.last_signed = signed;
    LoadResult {
        value: context.value[0],
        ok: context.ok,
    }
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_load_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut GfniMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 16 | 32 | 64)
    {
        return 0;
    }
    let mut value = if zero_upper != 0 {
        [0; 8]
    } else {
        state.vector_scratch
    };
    value[..(size / 8) as usize].copy_from_slice(&context.value[..(size / 8) as usize]);
    state.vector_scratch = value;
    1
}

#[cfg(target_arch = "x86_64")]
fn initial_registers(ordinal: usize) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K64};

    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x20)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0xA55A_3CC3_F00F_9696u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        mxcsr: 0x1F80,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        });
    }
    registers.gpr[2] = 0x3000 + ((ordinal & 0x0F) as u64) * 0x100;
    registers
}

#[cfg(target_arch = "x86_64")]
fn source_value(ordinal: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0x00FF_80FE_7F01_A55Au64.rotate_left((ordinal * 13 + word * 19) as u32)
            ^ (word as u64).wrapping_mul(0x1B11_011B_1B11_011B)
    })
}

#[cfg(target_arch = "x86_64")]
fn interpreter_success(
    function: &SmirFunction,
    initial: &crate::smir::lower::runtime::GuestRegs,
    source: [u64; 8],
    address: u64,
    memory_size: u32,
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(source) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    memory.load(address as usize, &bytes[..memory_size as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut expected = *initial;
    expected.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        expected.zmm[index].copy_from_slice(&value[..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    expected
}

#[cfg(target_arch = "x86_64")]
fn lower_native(function: &SmirFunction) -> (Vec<u8>, usize) {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .expect("lower native EVEX affine GFNI memory source");
    (
        lowerer
            .finalize()
            .expect("finalize native EVEX affine GFNI memory source"),
        result.entry_offset,
    )
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_matches_interpretation_and_e4nf_faults_even_with_all_mask_bits_clear() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("gfni")
    {
        eprintln!("skipping native EVEX affine GFNI: host lacks AVX-512F/BW or GFNI");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases = [
        (false, 0u8, 0u8, false, 1u8, false, 0x00u8, true),
        (true, 2, 15, false, 2, true, 0x63, false),
        (false, 1, 1, true, 1, false, 0xFF, true),
        (true, 2, 0, true, 0, false, 0xA5, false),
    ];
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut all_clear_faults = 0usize;
    for (ordinal, (inverse, ll, source1, broadcast, mask, zeroing, immediate, all_clear_mask)) in
        cases
            .into_iter()
            .filter(|(_, ll, ..)| *ll == 2 || has_vl)
            .enumerate()
    {
        let bytes = scanner_encoding(inverse, ll, source1, broadcast, mask, zeroing, immediate);
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = lift_bytes(&bytes, level);
            let sequence = sequence(&function);
            let (code, entry) = lower_native(&function);
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{bytes:02X?} {level:?}: {error:?}"));
            let source = source_value(ordinal);

            let mut context = GfniMemoryContext::new(source, true);
            let mut registers = initial_registers(ordinal);
            if mask != 0 {
                registers.k[usize::from(mask)] = if all_clear_mask {
                    1u64 << 63
                } else {
                    0xA55A_3CC3_F00F_9696
                };
            }
            let address = registers.gpr[2];
            registers.ctx = (&mut context as *mut GfniMemoryContext) as u64;
            if broadcast {
                registers.load_fn = scalar_load_helper as usize as u64;
            } else {
                registers.vec_load_fn = vector_load_helper as usize as u64;
            }
            let mut expected =
                interpreter_success(&function, &registers, source, address, sequence.memory_size);
            if !broadcast {
                expected.vector_scratch = std::array::from_fn(|word| {
                    if word < (sequence.memory_size / 8) as usize {
                        source[word]
                    } else {
                        0
                    }
                });
            }

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{bytes:02X?} {level:?}: success");
            assert_eq!(context.calls, 1, "{bytes:02X?} {level:?}");
            assert_eq!(context.last_addr, address, "{bytes:02X?} {level:?}");
            assert_eq!(
                context.last_size, sequence.memory_size,
                "{bytes:02X?} {level:?}"
            );
            assert_eq!(context.last_signed, 0, "{bytes:02X?} {level:?}");
            if !broadcast {
                assert_eq!(
                    context.last_index,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
                );
                assert_eq!(context.last_zero_upper, 1);
            }
            successes += 1;

            let mut fault_context = GfniMemoryContext::new(source, false);
            let mut fault_registers = initial_registers(ordinal ^ 0x55);
            if mask != 0 {
                fault_registers.k[usize::from(mask)] = if all_clear_mask {
                    1u64 << 63
                } else {
                    0xA55A_3CC3_F00F_9696
                };
            }
            let fault_address = fault_registers.gpr[2];
            fault_registers.ctx = (&mut fault_context as *mut GfniMemoryContext) as u64;
            if broadcast {
                fault_registers.load_fn = scalar_load_helper as usize as u64;
            } else {
                fault_registers.vec_load_fn = vector_load_helper as usize as u64;
            }
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{bytes:02X?} {level:?}: fault committed state"
            );
            assert_eq!(fault_context.calls, 1, "{bytes:02X?} {level:?}");
            assert_eq!(
                fault_context.last_addr, fault_address,
                "{bytes:02X?} {level:?}"
            );
            assert_eq!(
                fault_context.last_size, sequence.memory_size,
                "{bytes:02X?} {level:?}"
            );
            faults += 1;
            all_clear_faults += usize::from(all_clear_mask);
        }
    }
    assert!(successes >= 4);
    assert_eq!(faults, successes);
    assert!(all_clear_faults >= 2);
}
