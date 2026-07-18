//! Register, address, and control-register helpers

use crate::smir::lift::hexagon::*;
use std::collections::HashSet;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{HexDfOp, HexFpOp, HexFpRecipKind, OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

// Re-use the existing Hexagon decoder types
use crate::isa::hexagon::decode::{
    AddrMode, CmpKind, DecodedInsn, ExtendKind, MemOpKind, MemOpSrc, MemSign,
    MemWidth as HexMemWidth, ShiftKind,
};
// Direct opcode-level decoding for the ~900 scalar ops that decode to
// `DecodedInsn::Unknown` (handled only by the sem layer in cpu.rs). The lifter
// re-decodes such words via `decode_word` and emits SMIR for the regular
// scalar register ops; see `lift_unknown_op`.
use crate::isa::hexagon::opcode::{DecodedOp, Opcode, decode_word};

impl HexagonLifter {
    /// Create a new Hexagon lifter
    pub fn new(isa: crate::config::HexagonIsa) -> Self {
        HexagonLifter {
            isa,
            pending_hist: None,
            packet_producers: Vec::new(),
            prev_word_ended_packet: true,
            packet_start_pc: 0,
        }
    }


    /// Create a lifter with default ISA (V68)
    pub fn default_isa() -> Self {
        Self::new(crate::config::HexagonIsa::V68)
    }


    /// Convert Hexagon register to VReg
    pub(crate) fn hex_reg(&self, reg: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::R(reg)))
    }


    /// Resolve a new-value `.new` source: `field >> 1` is the back-distance
    /// (1 = most recently produced) into the current packet's GPR producers.
    /// Returns the resolved producer register if it is in range, else `None`.
    /// Mirrors `resolve_new_value` in the Hexagon interpreter (cpu.rs): the
    /// producers list is built by `lift_insn` as it lifts the packet's
    /// instructions in order, so by the time a new-value store/jump is lifted
    /// (the assembler always places it after its producer), the producer GPR is
    /// already present. `None` means the producer is out of range (no matching
    /// in-packet producer) — the caller must reject so we never store/compare a
    /// wrong register.
    pub(crate) fn resolve_new_value_src(&self, field: u8) -> Option<u8> {
        let back = (field >> 1) as usize;
        if back >= 1 && back <= self.packet_producers.len() {
            Some(self.packet_producers[self.packet_producers.len() - back])
        } else {
            None
        }
    }


    /// Convert Hexagon predicate register to VReg
    pub(crate) fn hex_pred(&self, pred: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::P(pred)))
    }


    /// Convert an HVX vector register V0..V31 to an SMIR vector VReg.
    pub(crate) fn hex_v(&self, n: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)))
    }


    /// Normalize a decoded HVX vector-pair field to its even architectural base.
    pub(crate) fn hex_v_pair_base(n: u8) -> u8 {
        n & !1
    }


    /// Convert an HVX vector predicate register Q0..Q3 to an SMIR vector VReg.
    pub(crate) fn hex_q(&self, n: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(n)))
    }


    /// Convert Hexagon memory width to SMIR memory width
    pub(crate) fn hex_mem_width(&self, width: HexMemWidth) -> MemWidth {
        match width {
            HexMemWidth::Byte => MemWidth::B1,
            HexMemWidth::Half => MemWidth::B2,
            HexMemWidth::Word => MemWidth::B4,
            HexMemWidth::Double => MemWidth::B8,
        }
    }


    /// Convert Hexagon sign extension mode
    pub(crate) fn hex_sign(&self, sign: MemSign) -> SignExtend {
        match sign {
            MemSign::Signed => SignExtend::Sign,
            MemSign::Unsigned => SignExtend::Zero,
        }
    }


    /// Convert Hexagon address mode to SMIR address
    pub(crate) fn hex_addr(&self, addr: &AddrMode, ctx: &mut LiftContext) -> Address {
        match addr {
            AddrMode::Offset { base, offset } => {
                let offset = ctx.extend_imm(*offset);
                Address::BaseOffset {
                    base: self.hex_reg(*base),
                    offset: offset as i64,
                    disp_size: DispSize::Auto,
                }
            }
            AddrMode::PostIncImm { base, offset: _ }
            | AddrMode::PostIncReg { base, .. }
            | AddrMode::PostIncBrev { base, .. }
            | AddrMode::PostIncCircImm { base, .. }
            | AddrMode::PostIncCircReg { base, .. } => {
                // Post-increment: use base address, increment handled separately.
                Address::Direct(self.hex_reg(*base))
            }
            AddrMode::GpOffset { offset } => {
                let offset = ctx.extend_imm(*offset);
                Address::GpRel { offset }
            }
            AddrMode::Abs { addr } => Address::Absolute(*addr as u64),
            AddrMode::RegScaled { base, index, shift } => Address::BaseIndexScale {
                base: Some(self.hex_reg(*base)),
                index: self.hex_reg(*index),
                scale: 1u8 << *shift,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            // `memX(Re=##U6)`: the absolute-set forms also write Re; the
            // interpreter handles that side effect (these reach the lifter only
            // via the rejecting `Load` arm below, which never calls `hex_addr`).
            AddrMode::AbsSet { addr, .. } => Address::Absolute(*addr as u64),
            // `memX(Ru<<#u2+##U6)`: scaled index plus an absolute displacement.
            AddrMode::IndexAbs { index, shift, addr } => Address::BaseIndexScale {
                base: None,
                index: self.hex_reg(*index),
                scale: 1u8 << *shift,
                disp: *addr as i32,
                disp_size: DispSize::Auto,
            },
        }
    }


    /// Modifier (`M0`/`M1`) register for `modsel` as a VReg.
    pub(crate) fn hex_mod(&self, modsel: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::M(modsel & 1)))
    }


    /// Circular-start (`CS0`/`CS1`) register for `modsel` as a VReg.
    pub(crate) fn hex_cs(&self, modsel: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::Cs(modsel & 1)))
    }


    /// Map a Hexagon control-register index to the SMIR `HexagonReg` that models
    /// it AS A PLAIN VALUE REGISTER, for the control-register PAIR transfers
    /// (`tfrcpp`/`tfrpcp`). The interpreter stores control regs in `c[0..32]`
    /// (see `HexagonRegisters::control`/`set_control` in cpu/state.rs):
    ///   C0=SA0  C1=LC0  C2=SA1  C3=LC1  C4=P3:0  C5=(reserved)  C6=M0  C7=M1
    ///   C8=USR  C9=PC   C10=UGP C11=GP  C12=CS0  C13=CS1
    /// Returns `Some(vreg)` only for indices the SMIR `HexagonRegState` models as
    /// a value register. Deliberately `None` for:
    ///   - C4 (P3:0): packed predicates, not a single modeled register (and it is
    ///     only ever the LOW half of the C5:C4 pair, whose high half is unmodeled);
    ///   - C5 (reserved), C10 (UGP): unmodeled in `HexagonRegState`;
    ///   - C9 (PC): modeled, but the program counter is NOT a plain data register
    ///     in the per-instruction value-move model — writing it (`tfrpcp` to C9:C8)
    ///     is a control transfer, and reading it depends on the packet-PC, neither
    ///     of which this value-move lift captures. So the C9:C8 pair is rejected.
    /// A pair transfer is lifted only when BOTH halves return `Some`.
    pub(crate) fn hex_creg_value(&self, idx: u8) -> Option<VReg> {
        let reg = match idx {
            0 => HexagonReg::Sa0,
            1 => HexagonReg::Lc0,
            2 => HexagonReg::Sa1,
            3 => HexagonReg::Lc1,
            6 => HexagonReg::M(0),
            7 => HexagonReg::M(1),
            8 => HexagonReg::Usr,
            11 => HexagonReg::Gp,
            12 => HexagonReg::Cs(0),
            13 => HexagonReg::Cs(1),
            _ => return None,
        };
        Some(VReg::Arch(ArchReg::Hexagon(reg)))
    }


    pub(crate) fn hex_creg_pair_values(&self, idx: u8) -> Option<(VReg, VReg)> {
        if idx & 1 != 0 {
            return None;
        }
        Some((self.hex_creg_value(idx)?, self.hex_creg_value(idx + 1)?))
    }


    /// Convert Hexagon shift kind to SMIR shift op
    pub(crate) fn hex_shift(&self, kind: ShiftKind) -> ShiftOp {
        match kind {
            ShiftKind::Lsl => ShiftOp::Lsl,
            ShiftKind::Lsr => ShiftOp::Lsr,
            ShiftKind::Asr => ShiftOp::Asr,
        }
    }


    /// Return `addr` shifted by `delta` bytes (for the high half of a `memd`
    /// predicated load/store, EA+4). Only the address modes that the predicated
    /// memory forms produce need handling: `Offset` / `RegScaled` (base+#imm,
    /// base+Rt<<sh) and `Abs` (absolute). For the others we fall back to a value
    /// that the high-half access still computes correctly relative to the base.
    pub(crate) fn offset_addr(&self, addr: &Address, delta: i64) -> Address {
        match addr {
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => Address::BaseOffset {
                base: *base,
                offset: offset + delta,
                disp_size: *disp_size,
            },
            Address::Direct(r) => Address::BaseOffset {
                base: *r,
                offset: delta,
                disp_size: DispSize::Auto,
            },
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                disp_size,
            } => Address::BaseIndexScale {
                base: *base,
                index: *index,
                scale: *scale,
                disp: disp + delta as i32,
                disp_size: *disp_size,
            },
            Address::GpRel { offset } => Address::GpRel {
                offset: offset + delta as i32,
            },
            Address::Absolute(a) => Address::Absolute((*a as i64 + delta) as u64),
            Address::PcRel {
                offset,
                disp_size,
                base,
            } => Address::PcRel {
                offset: offset + delta,
                disp_size: *disp_size,
                base: *base,
            },
            // Hexagon never produces x86 segment-relative addresses.
            Address::SegmentRel { .. } => addr.clone(),
        }
    }


    /// Convert Hexagon compare kind to SMIR condition
    pub(crate) fn hex_cmp_to_cond(&self, kind: CmpKind) -> Condition {
        match kind {
            CmpKind::Eq => Condition::Eq,
            CmpKind::Ne => Condition::Ne,
            CmpKind::Gt => Condition::Sgt,
            CmpKind::Gtu => Condition::Ugt,
            CmpKind::Lte => Condition::Sle,
            CmpKind::Lteu => Condition::Ule,
            CmpKind::Gte => Condition::Sge,
        }
    }


    /// Hexagon PC-relative control-flow offsets are already decoded, including
    /// constant extenders. The extender only changes the architectural base:
    /// extended branches use packet PC, ordinary branches use instruction PC.
    pub(crate) fn pcrel_target(&self, ctx: &mut LiftContext, addr: GuestAddr, offset: i32) -> GuestAddr {
        let base = if ctx.take_extended_imm().is_some() {
            self.packet_start_pc
        } else {
            addr
        };
        base.wrapping_add(offset as i64 as u64) & !0x3
    }


    /// TEST/AUDIT probe: re-scan the full Hexagon opcode table and report which
    /// NON-V6 (scalar) opcodes still lift to `Unsupported`. For each opcode the
    /// encoding table's `value` (all variable fields = 0) is used as a canonical
    /// instruction word and fed through `lift_insn`. Returns the de-duplicated
    /// sorted list of `(opcode_name, unsupported_mnemonic)` for opcodes whose
    /// canonical word fails to lift. HVX (`V6_*`) opcodes are skipped (they are a
    /// separate, complete subsystem). This is a coverage signal, not a semantic
    /// check — it tells us which scalar ops remain genuinely unhandled.
    pub fn audit_unlifted_scalar() -> Vec<(&'static str, String)> {
        use crate::isa::hexagon::opcode::{ENCODINGS_BY_ICLASS, ENCODINGS_MISC, opcode_name};
        let mut seen: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
        let mut out: Vec<(&'static str, String)> = Vec::new();
        let all = ENCODINGS_BY_ICLASS
            .iter()
            .flat_map(|t| t.iter())
            .chain(ENCODINGS_MISC.iter());
        for enc in all {
            let name = opcode_name(enc.opcode);
            if name.starts_with("V6_") {
                continue;
            }
            if !seen.insert(name) {
                continue;
            }
            let mut lifter = HexagonLifter::default_isa();
            let mut ctx = LiftContext::new(SourceArch::Hexagon);
            let word = enc.value;
            match lifter.lift_insn(0x1000, &word.to_le_bytes(), &mut ctx) {
                Ok(_) => {}
                Err(LiftError::Unsupported { mnemonic, .. }) => {
                    out.push((name, mnemonic));
                }
                Err(_) => {
                    out.push((name, "lift_error".to_string()));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}
