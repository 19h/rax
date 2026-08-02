//! Exact helper-backed packed EVEX FMA3 scalar-broadcast memory coverage.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::types::{MemWidth, SignExtend};

mod masked;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BroadcastFormat {
    F16,
    F32,
    F64,
}

impl BroadcastFormat {
    const ALL: [Self; 3] = [Self::F16, Self::F32, Self::F64];

    const fn elem(self) -> VecElementType {
        match self {
            Self::F16 => VecElementType::F16,
            Self::F32 => VecElementType::F32,
            Self::F64 => VecElementType::F64,
        }
    }

    const fn map(self) -> u8 {
        match self {
            Self::F16 => 6,
            Self::F32 | Self::F64 => 2,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::F64)
    }

    const fn memory_width(self) -> MemWidth {
        match self {
            Self::F16 => MemWidth::B2,
            Self::F32 => MemWidth::B4,
            Self::F64 => MemWidth::B8,
        }
    }

    const fn memory_size(self) -> usize {
        self.memory_width().bytes() as usize
    }

    fn scalar_bits(self, alternate: bool) -> u64 {
        match self {
            Self::F16 => u64::from(if alternate { 0x3E00u16 } else { 0x3800u16 }),
            Self::F32 => u64::from(if alternate {
                1.5f32.to_bits()
            } else {
                0.5f32.to_bits()
            }),
            Self::F64 => {
                if alternate {
                    1.5f64.to_bits()
                } else {
                    0.5f64.to_bits()
                }
            }
        }
    }

    fn role_vector(self, role: usize) -> [u64; 8] {
        let mut bytes = [0u8; 64];
        let element_bytes = self.memory_size();
        let lanes = bytes.len() / element_bytes;
        for lane in 0..lanes {
            let lane_bits = match self {
                Self::F16 => {
                    const VALUES: [[u16; 4]; 2] = [
                        [0x3C00, 0x4000, 0x4200, 0x4400],
                        [0x3800, 0x3E00, 0x4100, 0x4300],
                    ];
                    u64::from(VALUES[role][lane & 3])
                }
                Self::F32 => {
                    const VALUES: [[f32; 4]; 2] = [[1.0, 2.0, 3.0, 4.0], [0.5, 1.5, 2.5, 3.5]];
                    u64::from(VALUES[role][lane & 3].to_bits())
                }
                Self::F64 => {
                    const VALUES: [[f64; 4]; 2] = [[1.0, 2.0, 3.0, 4.0], [0.5, 1.5, 2.5, 3.5]];
                    VALUES[role][lane & 3].to_bits()
                }
            };
            bytes[lane * element_bytes..(lane + 1) * element_bytes]
                .copy_from_slice(&lane_bits.to_le_bytes()[..element_bytes]);
        }
        std::array::from_fn(|word| {
            u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BroadcastCase {
    opcode: u8,
    format: BroadcastFormat,
    width: VecWidth,
    form: MemoryForm,
}

impl BroadcastCase {
    const fn proxy(self) -> FmaMemoryCase {
        FmaMemoryCase {
            opcode: self.opcode,
            w: self.format.w(),
            width: self.width,
            form: self.form,
        }
    }

    const fn destination(self) -> u8 {
        self.proxy().destination()
    }

    const fn source1(self) -> u8 {
        self.proxy().source1()
    }

    const fn base(self) -> Option<u8> {
        self.proxy().base()
    }

    const fn index(self) -> Option<u8> {
        self.proxy().index()
    }

    const fn kind(self) -> X86FmaKind {
        self.proxy().kind()
    }

    const fn order(self) -> X86FmaOrder {
        self.proxy().order()
    }

    const fn evex_start(self) -> usize {
        if matches!(self.form, MemoryForm::FsAddr32Sib) {
            2
        } else {
            0
        }
    }

    fn bytes(self) -> Vec<u8> {
        let mut bytes = self.proxy().bytes();
        let evex = self.evex_start();
        bytes[evex + 1] = (bytes[evex + 1] & !0x07) | self.format.map();
        bytes[evex + 2] = (bytes[evex + 2] & !0x80) | (u8::from(self.format.w()) << 7);
        bytes[evex + 3] |= 0x10;
        bytes
    }

    fn stack_instruction(self) -> [u8; 7] {
        let bytes = self.bytes();
        let evex = self.evex_start();
        [
            0x62,
            (bytes[evex + 1] & 0x97) | 0x60,
            bytes[evex + 2] | 0x04,
            bytes[evex + 3] & 0x78,
            self.opcode,
            (bytes[evex + 5] & 0x38) | 0x04,
            0x24,
        ]
    }
}

fn all_cases() -> Vec<BroadcastCase> {
    let mut cases = Vec::new();
    for opcode in PACKED_OPCODES {
        for format in BroadcastFormat::ALL {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in MemoryForm::ALL {
                    cases.push(BroadcastCase {
                        opcode,
                        format,
                        width,
                        form,
                    });
                }
            }
        }
    }
    cases
}

fn native_cases() -> Vec<BroadcastCase> {
    let mut cases = Vec::new();
    for opcode in PACKED_OPCODES {
        for format in BroadcastFormat::ALL {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in MemoryForm::NATIVE {
                    cases.push(BroadcastCase {
                        opcode,
                        format,
                        width,
                        form,
                    });
                }
            }
        }
    }
    cases
}

fn lift_case(case: BroadcastCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&bytes).expect("EVEX broadcast FMA3 provenance"),
    );
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Load { .. }))
        .expect("packed EVEX FMA3 broadcast scalar load")
}

fn assert_exact_sequence(function: &SmirFunction, case: BroadcastCase) {
    let index = sequence_index(function);
    let ops = &function.blocks[0].ops[index..];
    assert_eq!(ops.len(), 4, "{case:?}: {ops:#?}");
    assert!(ops.iter().all(|op| op.guest_pc == PC), "{case:?}");

    let scalar = match &ops[0].kind {
        OpKind::Load {
            dst: scalar @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*width, case.format.memory_width(), "{case:?}");
            assert!(addr.is_x86_state_backed_shape(), "{case:?}: {addr:?}");
            *scalar
        }
        other => panic!("{case:?}: expected scalar Load, got {other:?}"),
    };
    assert_eq!(ops[0].x86_hint, None, "{case:?}");

    let broadcast = match &ops[1].kind {
        OpKind::VBroadcast {
            dst: broadcast @ VReg::Virtual(_),
            scalar: source,
            elem,
            lanes,
        } => {
            assert_eq!(*source, scalar, "{case:?}");
            assert_eq!(*elem, case.format.elem(), "{case:?}");
            assert_eq!(
                *lanes,
                case.width.lanes(case.format.elem()) as u8,
                "{case:?}"
            );
            *broadcast
        }
        other => panic!("{case:?}: expected VBroadcast, got {other:?}"),
    };
    assert_eq!(ops[1].x86_hint, None, "{case:?}");

    let raw = match (&ops[2].kind, case.format) {
        (
            OpKind::X86Fma(X86FmaOp {
                dst: raw @ VReg::Virtual(_),
                src1,
                src2,
                src3,
                mask,
                elem,
                kind,
                order,
                round,
                lanes,
            }),
            BroadcastFormat::F32 | BroadcastFormat::F64,
        ) => {
            assert_eq!(*src1, vector(case.destination(), case.width), "{case:?}");
            assert_eq!(*src2, vector(case.source1(), case.width), "{case:?}");
            assert_eq!(*src3, broadcast, "{case:?}");
            assert_eq!(*mask, None, "{case:?}");
            assert_eq!(*elem, case.format.elem(), "{case:?}");
            assert_eq!(*kind, case.kind(), "{case:?}");
            assert_eq!(*order, case.order(), "{case:?}");
            assert_eq!(*round, FpRoundMode::Dynamic, "{case:?}");
            assert_eq!(
                *lanes,
                case.width.lanes(case.format.elem()) as u8,
                "{case:?}"
            );
            *raw
        }
        (
            OpKind::X86FP16Fma {
                dst: raw @ VReg::Virtual(_),
                src1,
                src2,
                src3,
                mask,
                kind,
                order,
                round,
                lanes,
            },
            BroadcastFormat::F16,
        ) => {
            assert_eq!(*src1, vector(case.destination(), case.width), "{case:?}");
            assert_eq!(*src2, vector(case.source1(), case.width), "{case:?}");
            assert_eq!(*src3, broadcast, "{case:?}");
            assert_eq!(*mask, None, "{case:?}");
            assert_eq!(*kind, case.kind(), "{case:?}");
            assert_eq!(*order, case.order(), "{case:?}");
            assert_eq!(*round, FpRoundMode::Dynamic, "{case:?}");
            assert_eq!(
                *lanes,
                case.width.lanes(VecElementType::F16) as u8,
                "{case:?}"
            );
            *raw
        }
        (other, _) => panic!("{case:?}: unexpected FMA op {other:?}"),
    };
    assert_eq!(
        ops[2].x86_hint,
        Some(X86OpHint::EvexOp {
            map: if case.format == BroadcastFormat::F16 {
                X86VecMap::Map6
            } else {
                X86VecMap::Map0F38
            },
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode,
            width: case.width,
            w: case.format.w(),
        }),
        "{case:?}"
    );
    assert!(
        matches!(
            ops[3].kind,
            OpKind::VMov {
                dst,
                src,
                width,
            } if dst == vector(case.destination(), case.width)
                && src == raw
                && width == case.width
        ),
        "{case:?}: {:?}",
        ops[3].kind
    );
    assert_eq!(ops[3].x86_hint, None, "{case:?}");
}

fn lower(function: &SmirFunction, case: BroadcastCase) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512fp16,
        case.format == BroadcastFormat::F16,
        "{case:?}"
    );
    assert!(!requirements.needs_fma, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
            && (case.format != BroadcastFormat::F16 || std::is_x86_feature_detected!("avx512fp16")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(
        !x86_native_vector_features_supported_excluding(function, &excluded),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: broadcast FMA3 lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed EVEX broadcast FMA3"),
        result.entry_offset,
    )
}

#[test]
fn packed_evex_fma3_broadcast_classifier_exhaustively_rewrites_1_658_880_operands() {
    let mut accepted = 0usize;
    for opcode in PACKED_OPCODES {
        for format in BroadcastFormat::ALL {
            for (ll, width) in [
                (0, VecWidth::V128),
                (1, VecWidth::V256),
                (2, VecWidth::V512),
            ] {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        let p0 = (if destination & 8 == 0 { 0x80 } else { 0 })
                            | 0x60
                            | (if destination & 16 == 0 { 0x10 } else { 0 })
                            | format.map();
                        let p1 = (u8::from(format.w()) << 7) | (((!source1) & 0x0F) << 3) | 0x05;
                        let p2 = (ll << 5) | 0x10 | if source1 & 16 == 0 { 0x08 } else { 0 };
                        let bytes = [
                            0x62,
                            p0,
                            p1,
                            p2,
                            opcode,
                            0x40 | ((destination & 7) << 3) | 3,
                            DISP8,
                        ];
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_packed_fma3_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        let X86EvexPackedFma3MemoryReplay::Broadcast { stack_instruction } =
                            encoding.replay
                        else {
                            panic!("{bytes:02X?}: broadcast selected vector replay");
                        };
                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                        assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_eq!(encoding.writemask, None, "{bytes:02X?}");
                        assert!(!encoding.zeroing, "{bytes:02X?}");
                        assert_eq!(encoding.opcode, opcode, "{bytes:02X?}");
                        assert_eq!(encoding.w, format.w(), "{bytes:02X?}");
                        assert_eq!(encoding.needs_avx512vl, ll != 2, "{bytes:02X?}");
                        assert_eq!(
                            stack_instruction.as_slice(),
                            &[
                                0x62,
                                (p0 & 0x97) | 0x60,
                                p1 | 0x04,
                                p2 & 0x78,
                                opcode,
                                ((destination & 7) << 3) | 0x04,
                                0x24,
                            ],
                            "{bytes:02X?}"
                        );
                        accepted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 18 * 3 * 3 * 32 * 32);

    for format in BroadcastFormat::ALL {
        for opcode in 0..=u8::MAX {
            let bytes = [
                0x62,
                0xF0 | format.map(),
                (u8::from(format.w()) << 7) | 0x75,
                0x18,
                opcode,
                0x43,
                DISP8,
            ];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_packed_fma3_memory_encoding()
                    .is_some(),
                PACKED_OPCODES.contains(&opcode),
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn packed_evex_fma3_broadcast_stack_rewrites_match_llvm_23() {
    // LLVM 23.0.0:
    // vfmadd231ps xmm1,xmm2,dword ptr [rsp]{1to4}
    // vfmadd231pd ymm24,ymm25,qword ptr [rsp]{1to4}
    // vfmaddsub132ph zmm31,zmm30,word ptr [rsp]{1to32}
    let cases: [(&[u8], &[u8]); 3] = [
        (
            &[0x62, 0xF2, 0x6D, 0x18, 0xB8, 0x0B],
            &[0x62, 0xF2, 0x6D, 0x18, 0xB8, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x62, 0xB5, 0x30, 0xB8, 0x03],
            &[0x62, 0x62, 0xB5, 0x30, 0xB8, 0x04, 0x24],
        ),
        (
            &[0x62, 0x66, 0x0D, 0x50, 0x96, 0x3B],
            &[0x62, 0x66, 0x0D, 0x50, 0x96, 0x3C, 0x24],
        ),
    ];
    for (source, expected) in cases {
        let encoding = X86InstructionBytes::new(source)
            .unwrap()
            .evex_packed_fma3_memory_encoding()
            .unwrap_or_else(|| panic!("{source:02X?}"));
        let X86EvexPackedFma3MemoryReplay::Broadcast { stack_instruction } = encoding.replay else {
            panic!("{source:02X?}: broadcast selected vector replay");
        };
        assert_eq!(stack_instruction.as_slice(), expected, "{source:02X?}");
    }
}

#[test]
fn all_1134_broadcast_shapes_lift_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 18 * 3 * 3 * 7);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_sequence(&function, case);
            let index = sequence_index(&function);
            let (definitions, uses) = virtual_counts(&function);
            let sequence = x86_jit_evex_packed_fma3_memory_sequence(
                &function.blocks[0],
                index,
                true,
                &function.x86_instruction_bytes,
                &definitions,
                &uses,
            )
            .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(sequence.consumed, 4, "{level:?} {case:?}");
            assert_eq!(sequence.memory_offset, 0, "{level:?} {case:?}");
            assert_eq!(
                sequence.memory_size,
                case.format.memory_width().bytes(),
                "{level:?} {case:?}"
            );
            let X86EvexPackedFma3MemoryReplay::Broadcast { stack_instruction } =
                sequence.encoding.replay
            else {
                panic!("{level:?} {case:?}: broadcast selected vector replay");
            };
            assert_eq!(
                stack_instruction.as_slice(),
                case.stack_instruction(),
                "{level:?} {case:?}"
            );

            let (code, _) = lower(&function, case);
            let expected = case.stack_instruction();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            assert!(
                code.windows(5).any(|window| {
                    window == [0xBA, case.format.memory_width().bytes() as u8, 0, 0, 0]
                }),
                "{level:?} {case:?}: missing scalar helper size"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0x48, 0x8D, 0x64, 0x24, 0xF0]),
                "{level:?} {case:?}: missing 16-byte stack allocation"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0x48, 0x8D, 0x64, 0x24, 0x10]),
                "{level:?} {case:?}: missing 16-byte stack release"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 1134 * LEVELS.len());
}

#[test]
fn packed_evex_fma3_broadcast_rejects_reserved_encodings_and_non_owned_graphs() {
    let case = BroadcastCase {
        opcode: 0x98,
        format: BroadcastFormat::F32,
        width: VecWidth::V128,
        form: MemoryForm::Low,
    };
    let valid = case.bytes();
    let mut malformed = Vec::new();
    malformed.push(valid[..valid.len() - 1].to_vec());
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    register.truncate(6);
    malformed.push(register);
    for (byte_index, mask) in [(1, 0x01), (2, 0x01), (3, 0x80)] {
        let mut bytes = valid.clone();
        bytes[byte_index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut scalar = valid.clone();
    scalar[4] = 0x99;
    malformed.push(scalar);
    let mut fp16_w1 = BroadcastCase {
        format: BroadcastFormat::F16,
        ..case
    }
    .bytes();
    fp16_w1[2] |= 0x80;
    malformed.push(fp16_w1);
    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_fma3_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let base = lift_case(case);
    let scalar = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let broadcast = match base.blocks[0].ops[1].kind {
        OpKind::VBroadcast { dst, .. } => dst,
        _ => unreachable!(),
    };
    let raw = match base.blocks[0].ops[2].kind {
        OpKind::X86Fma(X86FmaOp { dst, .. }) => dst,
        _ => unreachable!(),
    };
    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_packed_fma3_memory_sequence(
            &base.blocks[0],
            0,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    let mut metadata_nonbroadcast = base.clone();
    let mut bytes = case.bytes();
    bytes[3] &= !0x10;
    metadata_nonbroadcast
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    let mut load_sign = base.clone();
    if let OpKind::Load { sign, .. } = &mut load_sign.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    let mut load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }
    let mut virtual_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    let mut scalar_reused = base.clone();
    scalar_reused.blocks[0].ops.push(SmirOp::new(
        OpId(4),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: crate::smir::ir::types::SrcOperand::Reg(scalar),
            width: crate::smir::ir::types::OpWidth::W64,
        },
    ));
    let mut broadcast_pc = base.clone();
    broadcast_pc.blocks[0].ops[1].guest_pc += 1;
    let mut broadcast_hint = base.clone();
    broadcast_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Aligned,
    ));
    let mut broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar: source, .. } = &mut broadcast_scalar.blocks[0].ops[1].kind {
        *source = VReg::Imm(0);
    }
    let mut broadcast_elem = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut broadcast_elem.blocks[0].ops[1].kind {
        *elem = VecElementType::F64;
    }
    let mut broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut broadcast_lanes.blocks[0].ops[1].kind {
        *lanes -= 1;
    }
    let mut broadcast_reused = base.clone();
    broadcast_reused.blocks[0].ops.push(SmirOp::new(
        OpId(4),
        PC + 1,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: broadcast,
            width: VecWidth::V128,
        },
    ));
    let mut fma_source = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_source.blocks[0].ops[2].kind {
        op.src3 = vector(3, VecWidth::V128);
    }
    let mut fma_mask = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_mask.blocks[0].ops[2].kind {
        op.mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(1))));
    }
    let mut raw_reused = base.clone();
    raw_reused.blocks[0].ops.push(SmirOp::new(
        OpId(4),
        PC + 1,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: raw,
            width: VecWidth::V128,
        },
    ));
    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(4),
        PC,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: vector(3, VecWidth::V128),
            width: VecWidth::V128,
        },
    ));
    let mut missing_result = base.clone();
    missing_result.blocks[0].ops.pop();

    let malformed = [
        ("missing metadata", missing_metadata),
        ("metadata non-broadcast", metadata_nonbroadcast),
        ("load hint", load_hint),
        ("load sign", load_sign),
        ("load width", load_width),
        ("virtual address", virtual_address),
        ("scalar reused", scalar_reused),
        ("broadcast PC", broadcast_pc),
        ("broadcast hint", broadcast_hint),
        ("broadcast scalar", broadcast_scalar),
        ("broadcast element", broadcast_elem),
        ("broadcast lanes", broadcast_lanes),
        ("broadcast reused", broadcast_reused),
        ("FMA source", fma_source),
        ("FMA mask", fma_mask),
        ("raw reused", raw_reused),
        ("same-PC tail", same_pc_tail),
        ("missing result", missing_result),
    ];
    for (name, function) in malformed {
        super::assert_rejected(name, &function);
    }
}

fn full_guest_regs(case: BroadcastCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        fs_base: 0x400,
        gs_base: 0x800,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        });
    }
    registers.zmm[usize::from(case.destination())] = case.format.role_vector(0);
    if case.source1() != case.destination() {
        registers.zmm[usize::from(case.source1())] = case.format.role_vector(1);
    }
    if let Some(base) = case.base() {
        registers.gpr[usize::from(base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x100;
    }
    if let Some(index) = case.index() {
        registers.gpr[usize::from(index)] = 0x300 + ((ordinal & 0x0F) as u64) * 0x20;
    }
    registers
}

fn memory_address(case: BroadcastCase, registers: &GuestRegs) -> u64 {
    let displacement = u64::from(DISP8) * case.format.memory_width().bytes() as u64;
    match case.form {
        MemoryForm::Low
        | MemoryForm::High
        | MemoryForm::DestinationSourceAlias
        | MemoryForm::ApxR16Base => {
            registers.gpr[usize::from(case.base().unwrap())].wrapping_add(displacement)
        }
        MemoryForm::FsAddr32Sib => {
            let offset = (registers.gpr[3] as u32)
                .wrapping_add((registers.gpr[6] as u32).wrapping_mul(2))
                .wrapping_add(displacement as u32);
            registers.fs_base.wrapping_add(u64::from(offset))
        }
        MemoryForm::RipRelative => {
            (PC + case.bytes().len() as u64).wrapping_add_signed(i64::from(DISP32))
        }
        MemoryForm::ApxR16R17Sib => registers.gpr[16]
            .wrapping_add(registers.gpr[17].wrapping_mul(2))
            .wrapping_add(displacement),
    }
}

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    scalar: u64,
    address: u64,
    format: BroadcastFormat,
) -> GuestRegs {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.fs_base = initial.fs_base;
        x86.gs_base = initial.gs_base;
        x86.apx_enabled = true;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    memory.load(
        address as usize,
        &scalar.to_le_bytes()[..format.memory_size()],
    );
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

#[test]
fn interpreter_o0_o1_o2_match_all_1134_broadcast_shapes_and_scalar_sources() {
    let cases = all_cases();
    assert_eq!(cases.len(), 18 * 3 * 3 * 7);
    let mut executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = full_guest_regs(case, ordinal);
        let address = memory_address(case, &initial);
        assert!(
            address + case.format.memory_size() as u64 <= 0x10000,
            "{case:?}: address {address:#x}"
        );
        let source = case.format.scalar_bits(false);
        let alternate_source = case.format.scalar_bits(true);
        let expected = interpreter_success(
            &optimize(lift_case(case), OptLevel::O0),
            &initial,
            source,
            address,
            case.format,
        );
        let alternate = interpreter_success(
            &optimize(lift_case(case), OptLevel::O0),
            &initial,
            alternate_source,
            address,
            case.format,
        );
        assert_ne!(
            expected.zmm[usize::from(case.destination())],
            alternate.zmm[usize::from(case.destination())],
            "{case:?}: scalar broadcast source did not affect the FMA result"
        );
        for level in LEVELS {
            let actual = interpreter_success(
                &optimize(lift_case(case), level),
                &initial,
                source,
                address,
                case.format,
            );
            assert_eq!(actual, expected, "{level:?} {case:?}");
            executions += 1;
        }
    }
    assert_eq!(executions, 1134 * LEVELS.len());
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Default)]
struct ScalarMemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn scalar_load_helper(
    context: *mut ScalarMemoryContext,
    addr: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_broadcast_fma3_matches_interpretation_and_faults_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native packed EVEX FMA3 broadcast: host lacks AVX-512F/BW");
        return;
    }

    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    let cases: Vec<_> = native_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .filter(|case| case.format != BroadcastFormat::F16 || has_fp16)
        .collect();
    assert!(!cases.is_empty());
    let expected_executions = cases.len() * NATIVE_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in NATIVE_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let scalar = case.format.scalar_bits(ordinal & 1 != 0);
            let mut context = ScalarMemoryContext {
                value: scalar,
                ok: 1,
                ..ScalarMemoryContext::default()
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = memory_address(case, &registers);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as usize as u64;
            let mut expected =
                interpreter_success(&function, &registers, scalar, address, case.format);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_size,
                case.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
            successes += 1;

            let mut fault_context = ScalarMemoryContext {
                value: scalar ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut fault_registers = full_guest_regs(case, ordinal ^ 0x55);
            let fault_address = memory_address(case, &fault_registers);
            fault_registers.ctx = (&mut fault_context as *mut ScalarMemoryContext) as u64;
            fault_registers.load_fn = scalar_load_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: fault committed state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}");
            assert_eq!(fault_context.last_addr, fault_address, "{level:?} {case:?}");
            assert_eq!(
                fault_context.last_size,
                case.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(fault_context.last_signed, 0, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
