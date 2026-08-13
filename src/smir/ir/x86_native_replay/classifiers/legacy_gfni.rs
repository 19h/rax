//! Exact register-only legacy GFNI replay.

use std::collections::{HashMap, HashSet};

use super::super::X86VexGfniMemoryKind;
use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, OpWidth, ShiftOp, SignExtend, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

/// Decoded architectural operands of one exact register-only legacy GFNI
/// instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyGfniReplay {
    pub(crate) kind: X86VexGfniMemoryKind,
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) immediate: Option<u8>,
}

/// Expected block-wide definition/use counts for one temporary elided by
/// exact native replay.
pub(crate) type X86LegacyGfniVirtualRequirement = (VReg, usize, usize);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

struct ExactGfniShape<'a> {
    ops: &'a [SmirOp],
    cursor: usize,
    definitions: HashSet<VReg>,
    definition_order: Vec<VReg>,
    uses: HashMap<VReg, usize>,
}

impl<'a> ExactGfniShape<'a> {
    fn new(ops: &'a [SmirOp]) -> Self {
        Self {
            ops,
            cursor: 0,
            definitions: HashSet::new(),
            definition_order: Vec::new(),
            uses: HashMap::new(),
        }
    }

    fn operation(&mut self) -> Option<&'a SmirOp> {
        let operation = self.ops.get(self.cursor)?;
        self.cursor += 1;
        operation.x86_hint.is_none().then_some(operation)
    }

    fn define(&mut self, register: VReg) -> Option<VReg> {
        if !matches!(register, VReg::Virtual(_)) || !self.definitions.insert(register) {
            return None;
        }
        self.definition_order.push(register);
        Some(register)
    }

    fn consume(&mut self, register: VReg) -> Option<()> {
        if matches!(register, VReg::Virtual(_)) {
            if !self.definitions.contains(&register) {
                return None;
            }
            *self.uses.entry(register).or_insert(0) += 1;
        }
        Some(())
    }

    fn mov_immediate(&mut self, expected: u8) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(value),
                width: OpWidth::W64,
            } if value == i64::from(expected) => dst,
            _ => return None,
        };
        self.define(destination)
    }

    fn broadcast(&mut self, scalar: VReg) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::VBroadcast {
                dst,
                scalar: actual_scalar,
                elem: VecElementType::I8,
                lanes: 16,
            } if actual_scalar == scalar => dst,
            _ => return None,
        };
        self.consume(scalar)?;
        self.define(destination)
    }

    fn splat(&mut self, immediate: u8) -> Option<VReg> {
        let scalar = self.mov_immediate(immediate)?;
        self.broadcast(scalar)
    }

    fn and(&mut self, source1: VReg, source2: VReg) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::VAnd {
                dst,
                src1,
                src2,
                width: VecWidth::V128,
            } if src1 == source1 && src2 == source2 => dst,
            _ => return None,
        };
        self.consume(source1)?;
        self.consume(source2)?;
        self.define(destination)
    }

    fn or(&mut self, source1: VReg, source2: VReg) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::VOr {
                dst,
                src1,
                src2,
                width: VecWidth::V128,
            } if src1 == source1 && src2 == source2 => dst,
            _ => return None,
        };
        self.consume(source1)?;
        self.consume(source2)?;
        self.define(destination)
    }

    fn xor(&mut self, source1: VReg, source2: VReg) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::VXor {
                dst,
                src1,
                src2,
                width: VecWidth::V128,
            } if src1 == source1 && src2 == source2 => dst,
            _ => return None,
        };
        self.consume(source1)?;
        self.consume(source2)?;
        self.define(destination)
    }

    fn subtract(&mut self, source1: VReg, source2: VReg) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::VSub {
                dst,
                src1,
                src2,
                elem: VecElementType::I8,
                lanes: 16,
            } if src1 == source1 && src2 == source2 => dst,
            _ => return None,
        };
        self.consume(source1)?;
        self.consume(source2)?;
        self.define(destination)
    }

    fn shift(&mut self, source: VReg, amount: u8, shift: ShiftOp) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::VShift {
                dst,
                src,
                amount: SrcOperand::Imm(actual_amount),
                shift: actual_shift,
                elem: VecElementType::I8,
                lanes: 16,
            } if src == source && actual_amount == i64::from(amount) && actual_shift == shift => {
                dst
            }
            _ => return None,
        };
        self.consume(source)?;
        self.define(destination)
    }

    fn byte_shuffle(&mut self, source: VReg, control: VReg) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::VByteShuffle {
                dst,
                src,
                control: actual_control,
                lanes: 16,
                block_lanes: 8,
            } if src == source && actual_control == control => dst,
            _ => return None,
        };
        self.consume(source)?;
        self.consume(control)?;
        self.define(destination)
    }

    fn extract_byte(&mut self, vector: VReg, lane: u8) -> Option<VReg> {
        let operation = self.operation()?;
        let destination = match operation.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem: VecElementType::I8,
                sign: SignExtend::Zero,
            } if vec == vector && actual_lane == lane => dst,
            _ => return None,
        };
        self.consume(vector)?;
        self.define(destination)
    }

    fn insert_byte(&mut self, vector: VReg, scalar: VReg, lane: u8) -> Option<()> {
        let operation = self.operation()?;
        if !matches!(
            operation.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem: VecElementType::I8,
            } if dst == vector
                && vec == vector
                && actual_scalar == scalar
                && actual_lane == lane
        ) {
            return None;
        }
        self.consume(vector)?;
        self.consume(scalar)
    }

    fn gf_multiply(&mut self, source1: VReg, source2: VReg, optimized: bool) -> Option<VReg> {
        let zero = self.splat(0)?;
        let one = self.splat(1)?;
        let reduction_polynomial = self.splat(0x1B)?;
        let mut result = zero;
        let mut multiplicand = source1;
        let mut multiplier = source2;

        for round in 0..8 {
            let multiplier_lsb = self.and(multiplier, one)?;
            let multiplier_mask = self.subtract(zero, multiplier_lsb)?;
            let contribution = self.and(multiplicand, multiplier_mask)?;
            result = self.xor(result, contribution)?;

            // O1/O2 remove the six dead state updates after the eighth result
            // contribution; O0 retains the exact lifter expansion.
            if optimized && round == 7 {
                continue;
            }
            let carry = self.shift(multiplicand, 7, ShiftOp::Lsr)?;
            let carry_mask = self.subtract(zero, carry)?;
            let reduction = self.and(carry_mask, reduction_polynomial)?;
            let shifted = self.shift(multiplicand, 1, ShiftOp::Lsl)?;
            multiplicand = self.xor(shifted, reduction)?;
            multiplier = self.shift(multiplier, 1, ShiftOp::Lsr)?;
        }
        Some(result)
    }

    fn gf_inverse(&mut self, source: VReg, optimized: bool) -> Option<VReg> {
        let mut power = self.gf_multiply(source, source, optimized)?;
        let mut result = power;
        for _ in 0..6 {
            power = self.gf_multiply(power, power, optimized)?;
            result = self.gf_multiply(result, power, optimized)?;
        }
        Some(result)
    }

    fn gf_affine(
        &mut self,
        source: VReg,
        matrix: VReg,
        immediate: u8,
        inverse: bool,
        optimized: bool,
    ) -> Option<VReg> {
        let input = if inverse {
            self.gf_inverse(source, optimized)?
        } else {
            source
        };
        let zero = self.splat(0)?;
        let one = self.splat(1)?;
        let mut result = zero;

        for output_bit in 0..8u8 {
            let control = self.splat(7 - output_bit)?;
            let matrix_row = self.byte_shuffle(matrix, control)?;
            let mut parity = self.and(matrix_row, input)?;
            for amount in [4, 2, 1] {
                let high = self.shift(parity, amount, ShiftOp::Lsr)?;
                parity = self.xor(parity, high)?;
            }
            parity = self.and(parity, one)?;
            if output_bit != 0 {
                parity = self.shift(parity, output_bit, ShiftOp::Lsl)?;
            }
            result = self.or(result, parity)?;
        }

        let constant = self.splat(immediate)?;
        self.xor(result, constant)
    }

    fn legacy_commit(&mut self, destination: VReg, raw: VReg) -> Option<()> {
        let mut scalars = Vec::with_capacity(16);
        for lane in 0..16 {
            scalars.push((lane, self.extract_byte(raw, lane)?));
        }
        for (lane, scalar) in scalars {
            self.insert_byte(destination, scalar, lane)?;
        }
        Some(())
    }

    fn finish(self) -> Option<Vec<X86LegacyGfniVirtualRequirement>> {
        if self.cursor != self.ops.len() {
            return None;
        }
        let uses = self.uses;
        Some(
            self.definition_order
                .into_iter()
                .map(|register| (register, 1, uses.get(&register).copied().unwrap_or(0)))
                .collect(),
        )
    }
}

fn exact_shape(
    ops: &[SmirOp],
    replay: X86LegacyGfniReplay,
    optimized: bool,
) -> Option<Vec<X86LegacyGfniVirtualRequirement>> {
    let mut shape = ExactGfniShape::new(ops);
    let destination = xmm(replay.destination);
    let source = xmm(replay.source);
    let raw = match replay.kind {
        X86VexGfniMemoryKind::Multiply if replay.immediate.is_none() => {
            shape.gf_multiply(destination, source, optimized)?
        }
        X86VexGfniMemoryKind::Affine => {
            shape.gf_affine(destination, source, replay.immediate?, false, optimized)?
        }
        X86VexGfniMemoryKind::AffineInverse => {
            shape.gf_affine(destination, source, replay.immediate?, true, optimized)?
        }
        _ => return None,
    };
    shape.legacy_commit(destination, raw)?;
    shape.finish()
}

/// Validate the complete O0/O1/O2 semantic graph emitted for one
/// register-only legacy GFNI instruction. Each returned tuple is `(virtual
/// register, definitions, uses)` so the grouping layer can prove that every
/// elided reconstruction temporary is confined to this source instruction.
/// Validation is O(K) time and O(V) space for K operations and V virtual
/// registers; the current maximum is 1,260 operations.
pub(crate) fn x86_legacy_gfni_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyGfniReplay,
) -> Option<Vec<X86LegacyGfniVirtualRequirement>> {
    exact_shape(ops, replay, false).or_else(|| exact_shape(ops, replay, true))
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy GFNI instruction.
    ///
    /// The admitted set is exactly `GF2P8MULB`, `GF2P8AFFINEQB`, and
    /// `GF2P8AFFINEINVQB` with one mandatory 66H prefix followed by one
    /// optional final REX prefix. REX.R/B extend the XMM operands; REX.W/X are
    /// ignored architecturally and retained in the replay bytes. Memory,
    /// duplicate/reordered/other legacy prefixes, REX2/VEX/EVEX, truncated
    /// instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_gfni_replay(&self) -> Option<X86LegacyGfniReplay> {
        let (rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (Some(*rex), tail),
            [0x66, tail @ ..] => (None, tail),
            _ => return None,
        };
        let (kind, modrm, immediate) = match tail {
            [0x0F, 0x38, 0xCF, modrm] => (X86VexGfniMemoryKind::Multiply, *modrm, None),
            [0x0F, 0x3A, opcode @ (0xCE | 0xCF), modrm, immediate] => (
                if *opcode == 0xCE {
                    X86VexGfniMemoryKind::Affine
                } else {
                    X86VexGfniMemoryKind::AffineInverse
                },
                *modrm,
                Some(*immediate),
            ),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let rex = rex.unwrap_or(0);
        Some(X86LegacyGfniReplay {
            kind,
            destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (modrm & 7) | ((rex & 0x01) << 3),
            immediate,
        })
    }
}
