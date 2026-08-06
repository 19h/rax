//! Exact helper-backed EVEX `VMOVD`/`VMOVQ`/`VMOVW` memory coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexScalarMoveMemoryEncoding, X86EvexScalarMoveMemoryKind, X86InstructionBytes,
};

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntegerSelector {
    DLoad,
    DStore,
    QLoad6e,
    QStore7e,
    QLoad7e,
    QStoreD6,
    W0Load,
    W1Load,
    W0Store,
    W1Store,
}

impl IntegerSelector {
    const ALL: [Self; 10] = [
        Self::DLoad,
        Self::DStore,
        Self::QLoad6e,
        Self::QStore7e,
        Self::QLoad7e,
        Self::QStoreD6,
        Self::W0Load,
        Self::W1Load,
        Self::W0Store,
        Self::W1Store,
    ];

    const fn kind(self) -> X86EvexScalarMoveMemoryKind {
        match self {
            Self::DLoad | Self::QLoad6e | Self::QLoad7e | Self::W0Load | Self::W1Load => {
                X86EvexScalarMoveMemoryKind::Load
            }
            Self::DStore | Self::QStore7e | Self::QStoreD6 | Self::W0Store | Self::W1Store => {
                X86EvexScalarMoveMemoryKind::Store
            }
        }
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::DLoad | Self::DStore => VecElementType::I32,
            Self::QLoad6e | Self::QStore7e | Self::QLoad7e | Self::QStoreD6 => VecElementType::I64,
            Self::W0Load | Self::W1Load | Self::W0Store | Self::W1Store => VecElementType::I16,
        }
    }

    const fn map(self) -> u8 {
        match self.elem() {
            VecElementType::I16 => 5,
            VecElementType::I32 | VecElementType::I64 => 1,
            _ => unreachable!(),
        }
    }

    const fn pp(self) -> u8 {
        match self {
            Self::QLoad7e => 2,
            _ => 1,
        }
    }

    const fn w(self) -> bool {
        matches!(
            self,
            Self::QLoad6e
                | Self::QStore7e
                | Self::QLoad7e
                | Self::QStoreD6
                | Self::W1Load
                | Self::W1Store
        )
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::DLoad | Self::QLoad6e | Self::W0Load | Self::W1Load => 0x6E,
            Self::DStore | Self::QStore7e | Self::QLoad7e | Self::W0Store | Self::W1Store => 0x7E,
            Self::QStoreD6 => 0xD6,
        }
    }

    const fn memory_width(self) -> MemWidth {
        match self.elem() {
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 => MemWidth::B4,
            VecElementType::I64 => MemWidth::B8,
            _ => unreachable!(),
        }
    }

    const fn needs_avx512fp16(self) -> bool {
        self.map() == 5
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntegerCase {
    selector: IntegerSelector,
    vector: u8,
    base: u8,
}

impl IntegerCase {
    fn bytes(self) -> Vec<u8> {
        assert!(self.vector < 32 && self.base < 16);
        let low_base = self.base & 7;
        let mod_bits = if low_base == 5 { 0x40 } else { 0 };
        let mut bytes = vec![
            0x62,
            (if self.vector & 8 == 0 { 0x80 } else { 0 })
                | 0x40
                | (if self.base & 8 == 0 { 0x20 } else { 0 })
                | (if self.vector & 16 == 0 { 0x10 } else { 0 })
                | self.selector.map(),
            (u8::from(self.selector.w()) << 7) | 0x7C | self.selector.pp(),
            0x08,
            self.selector.opcode(),
            mod_bits | ((self.vector & 7) << 3) | low_base,
        ];
        if low_base == 4 {
            bytes.push(0x24);
        }
        if low_base == 5 {
            bytes.push(0);
        }
        bytes
    }

    fn stack_instruction(self) -> X86InstructionBytes {
        let bytes = self.bytes();
        let start = bytes.iter().position(|byte| *byte == 0x62).unwrap();
        let p0 = bytes[start + 1];
        let p1 = bytes[start + 2];
        let p2 = bytes[start + 3];
        let opcode = bytes[start + 4];
        let modrm = bytes[start + 5];
        X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2,
            opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])
        .unwrap()
    }

    fn expected_encoding(self) -> X86EvexScalarMoveMemoryEncoding {
        X86EvexScalarMoveMemoryEncoding {
            kind: self.selector.kind(),
            elem: self.selector.elem(),
            vector: self.vector,
            writemask: None,
            zeroing: false,
            map: self.selector.map(),
            pp: self.selector.pp(),
            w: self.selector.w(),
            ll: 0,
            opcode: self.selector.opcode(),
            memory_width: self.selector.memory_width(),
            stack_instruction: self.stack_instruction(),
            needs_avx512fp16: self.selector.needs_avx512fp16(),
        }
    }
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    function_from_bytes(bytes, bytes)
}

fn lift_case(case: IntegerCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_graph(&function, case);
    function
}

fn sequence_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitEvexScalarMoveMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_scalar_move_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn sequence_index(function: &SmirFunction) -> usize {
    usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ))
}

fn exact_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexScalarMoveMemorySequence> {
    sequence_at(function, sequence_index(function), allow_mem)
}

fn assert_exact_graph(function: &SmirFunction, case: IntegerCase) {
    let index = sequence_index(function);
    let ops = &function.blocks[0].ops[index..];
    let expected_consumed = match case.selector.kind() {
        X86EvexScalarMoveMemoryKind::Load => 4,
        X86EvexScalarMoveMemoryKind::Store => 2,
    };
    assert_eq!(ops.len(), expected_consumed, "{case:?}: {ops:#?}");
    match case.selector.kind() {
        X86EvexScalarMoveMemoryKind::Load => {
            let loaded = match &ops[0].kind {
                OpKind::Load {
                    dst,
                    width,
                    sign: SignExtend::Zero,
                    ..
                } => {
                    assert_eq!(*width, case.selector.memory_width(), "{case:?}");
                    *dst
                }
                other => panic!("{case:?}: expected scalar load, got {other:?}"),
            };
            let zero = match ops[1].kind {
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                } => dst,
                ref other => panic!("{case:?}: expected zero initializer, got {other:?}"),
            };
            assert_ne!(loaded, zero, "{case:?}");
            assert!(matches!(
                ops[2].kind,
                OpKind::VBroadcast {
                    dst,
                    scalar,
                    elem,
                    lanes: 1,
                } if dst == xmm(case.vector) && scalar == zero && elem == case.selector.elem()
            ));
            assert!(matches!(
                ops[3].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: 0,
                    elem,
                } if dst == xmm(case.vector)
                    && vec == xmm(case.vector)
                    && scalar == loaded
                    && elem == case.selector.elem()
            ));
        }
        X86EvexScalarMoveMemoryKind::Store => {
            let scalar = match ops[0].kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: 0,
                    elem,
                    sign: SignExtend::Zero,
                } if vec == xmm(case.vector) && elem == case.selector.elem() => dst,
                ref other => panic!("{case:?}: expected lane extraction, got {other:?}"),
            };
            assert!(matches!(
                ops[1].kind,
                OpKind::Store { src, width, .. }
                    if src == scalar && width == case.selector.memory_width()
            ));
        }
    }
    assert!(
        ops.iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none())
    );
    assert_eq!(
        exact_sequence(function, true),
        Some(X86JitEvexScalarMoveMemorySequence {
            consumed: expected_consumed,
            address_offset: match case.selector.kind() {
                X86EvexScalarMoveMemoryKind::Load => 0,
                X86EvexScalarMoveMemoryKind::Store => 1,
            },
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(exact_sequence(function, false), None, "{case:?}");
}

fn assert_feature_requirements(function: &SmirFunction, case: IntegerCase) {
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
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512fp16,
        case.selector.needs_avx512fp16(),
        "{case:?}"
    );
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512er, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
}

fn lower_case(function: &SmirFunction, case: IntegerCase) -> (Vec<u8>, usize) {
    assert_exact_graph(function, case);
    assert_feature_requirements(function, case);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed lowering failed: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    let code = lowerer
        .finalize()
        .expect("finalize EVEX integer scalar move");
    let stack = case.stack_instruction();
    assert!(
        code.windows(stack.as_slice().len())
            .any(|window| window == stack.as_slice()),
        "{case:?}: missing exact stack replay {:02X?}",
        stack.as_slice()
    );
    let size_register = match case.selector.kind() {
        X86EvexScalarMoveMemoryKind::Load => 0xBA,
        X86EvexScalarMoveMemoryKind::Store => 0xB9,
    };
    let helper_size = [
        size_register,
        case.selector.memory_width().bytes() as u8,
        0,
        0,
        0,
    ];
    assert!(
        code.windows(helper_size.len())
            .any(|window| window == helper_size),
        "{case:?}: missing exact helper width"
    );
    (code, result.entry_offset)
}

fn full_registers(case: IntegerCase, seed: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x20)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x195)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: 0x1F80 | ((seed as u32) & 0x3F) | (((seed as u32) & 3) << 13),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        apx_enabled: 1,
        ..GuestRegs::default()
    };
    registers.gpr[usize::from(case.base)] = MEMORY_ADDRESS;
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (seed as u64).wrapping_mul(0x8040_2010_0804_0201)
        });
    }
    registers
}
