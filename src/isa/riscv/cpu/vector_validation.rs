//! Architectural encoding and register-group validation for RVV data operations.
//!
//! These checks run before the data-path dispatcher mutates architectural state.
//! Keeping them together makes the direct interpreter and the opaque SMIR/JIT
//! helper paths reject the same reserved encodings at the same guest frontier.

use super::{Insn, Op, RiscVCpu, Trap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Emul {
    numerator: u8,
    denominator: u8,
}

impl Emul {
    fn from_vtype(vtype: u64) -> Option<Self> {
        match vtype & 0x7 {
            0b000 => Some(Self::new(1, 1)),
            0b001 => Some(Self::new(2, 1)),
            0b010 => Some(Self::new(4, 1)),
            0b011 => Some(Self::new(8, 1)),
            0b101 => Some(Self::new(1, 8)),
            0b110 => Some(Self::new(1, 4)),
            0b111 => Some(Self::new(1, 2)),
            0b100 => None,
            _ => None,
        }
    }

    fn new(mut numerator: u8, mut denominator: u8) -> Self {
        while numerator % 2 == 0 && denominator % 2 == 0 {
            numerator /= 2;
            denominator /= 2;
        }
        Self {
            numerator,
            denominator,
        }
    }

    fn widen(self) -> Self {
        Self::new(self.numerator * 2, self.denominator)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterGroup {
    first: u8,
    count: u8,
}

impl RegisterGroup {
    fn for_emul(first: u8, emul: Emul) -> Option<Self> {
        // RVV reserves any instruction whose effective register group is larger
        // than eight vector registers.
        if emul.numerator > 8 * emul.denominator {
            return None;
        }

        // Fractional EMUL occupies a fraction of one named architectural
        // register and therefore has no multi-register alignment constraint.
        let count = if emul.numerator < emul.denominator {
            1
        } else {
            if emul.numerator % emul.denominator != 0 {
                return None;
            }
            emul.numerator / emul.denominator
        };
        if first % count != 0 || first.checked_add(count)? > 32 {
            return None;
        }
        Some(Self { first, count })
    }

    fn overlaps(self, other: Self) -> bool {
        let self_end = self.first + self.count;
        let other_end = other.first + other.count;
        self.first < other_end && other.first < self_end
    }
}

#[inline]
fn illegal(insn: &Insn) -> Trap {
    Trap::illegal(insn.raw)
}

fn current_lmul(cpu: &RiscVCpu, insn: &Insn) -> Result<Emul, Trap> {
    Emul::from_vtype(cpu.vtype).ok_or_else(|| illegal(insn))
}

fn same_width_group(cpu: &RiscVCpu, insn: &Insn, first: u8) -> Result<RegisterGroup, Trap> {
    RegisterGroup::for_emul(first, current_lmul(cpu, insn)?).ok_or_else(|| illegal(insn))
}

fn validate_slide_up(cpu: &RiscVCpu, insn: &Insn) -> Result<(), Trap> {
    let destination = same_width_group(cpu, insn, insn.rd)?;
    let source = same_width_group(cpu, insn, insn.rs2)?;
    if destination.overlaps(source) {
        return Err(illegal(insn));
    }
    Ok(())
}

fn validate_narrowing(cpu: &RiscVCpu, insn: &Insn) -> Result<(), Trap> {
    let lmul = current_lmul(cpu, insn)?;
    let destination = RegisterGroup::for_emul(insn.rd, lmul).ok_or_else(|| illegal(insn))?;
    let source = RegisterGroup::for_emul(insn.rs2, lmul.widen()).ok_or_else(|| illegal(insn))?;

    // A narrow destination may overlap the lowest-numbered part of its wide
    // source group. Any other overlap is reserved.
    if destination.overlaps(source) && destination.first != source.first {
        return Err(illegal(insn));
    }
    Ok(())
}

fn is_vector_fp_encoding(insn: &Insn) -> bool {
    // OPFVV and OPFVF are the complete floating-point classes under OP-V.
    // Classifying the encoding, rather than maintaining an operation whitelist,
    // also covers exact operations and future decoded members of these classes.
    insn.raw & 0x7f == 0x57 && matches!(insn.funct3, 0b001 | 0b101)
}

pub(super) fn validate(cpu: &RiscVCpu, insn: &Insn, vm: bool) -> Result<(), Trap> {
    // All vector floating-point instructions consult frm, including operations
    // whose numeric result is independent of rounding and instructions with a
    // fixed RTZ/ROD behavior. Architectural frm encodings 5, 6, and 7 are
    // reserved; frm=7 is not a second level of dynamic selection.
    if is_vector_fp_encoding(insn) && cpu.frm() > 4 {
        return Err(illegal(insn));
    }

    match insn.op {
        Op::Vmsbf | Op::Vmsif | Op::Vmsof => {
            if cpu.vstart != 0 || insn.rd == insn.rs2 || (!vm && insn.rd == 0) {
                return Err(illegal(insn));
            }
        }
        Op::Vadc | Op::Vsbc => {
            if vm || insn.rd == 0 {
                return Err(illegal(insn));
            }
        }
        Op::Vslideup | Op::Vslide1up | Op::Vfslide1up => {
            validate_slide_up(cpu, insn)?;
        }
        Op::Vnsrl
        | Op::Vnsra
        | Op::Vnclipu
        | Op::Vnclip
        | Op::VfncvtXuF
        | Op::VfncvtXF
        | Op::VfncvtFXu
        | Op::VfncvtFX
        | Op::VfncvtFF
        | Op::VfncvtRodFF
        | Op::VfncvtRtzXuF
        | Op::VfncvtRtzXF => validate_narrowing(cpu, insn)?,
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::riscv::{FlatMemory, Isa, RiscVConfig, RiscVExit, Xlen, decode};

    const E8_M1: u64 = 0x00;
    const E32_M1: u64 = 0x10;

    fn op_v(funct6: u32, vm: u32, vs2: u32, src: u32, funct3: u32, vd: u32) -> u32 {
        (funct6 << 26) | (vm << 25) | (vs2 << 20) | (src << 15) | (funct3 << 12) | (vd << 7) | 0x57
    }

    fn decoded(raw: u32) -> Insn {
        let insn = decode(raw, Xlen::Rv64, &Isa::rv64gc());
        assert_ne!(insn.op, Op::Illegal, "test encoding {raw:08x} is illegal");
        insn
    }

    fn cpu(vtype: u64, vl: u64, vstart: u64, frm: u8) -> RiscVCpu {
        let mut cpu = RiscVCpu::new(RiscVConfig::rv64gc(), Box::new(FlatMemory::new(0, 0x2000)));
        cpu.set_vl_vtype(vl, vtype);
        cpu.set_vstart(vstart);
        cpu.set_fcsr(u32::from(frm) << 5);
        for register in 0..32u8 {
            cpu.set_x(register, 0x1020_3040_5060_7080 ^ u64::from(register));
            cpu.set_f(register, 0xffff_ffff_3f80_0000 + u64::from(register));
            cpu.set_vreg(register, &[register.wrapping_mul(7); 16]);
        }
        cpu
    }

    fn architectural_state(
        cpu: &RiscVCpu,
    ) -> (
        [u64; 32],
        [u64; 32],
        [[u8; 16]; 32],
        u32,
        u64,
        u64,
        u64,
        u64,
    ) {
        (
            std::array::from_fn(|index| cpu.x(index as u8)),
            std::array::from_fn(|index| cpu.f(index as u8)),
            std::array::from_fn(|index| cpu.vreg(index as u8)),
            cpu.fcsr(),
            cpu.vl(),
            cpu.vtype(),
            cpu.vstart(),
            cpu.vcsr(),
        )
    }

    fn assert_illegal(raw: u32, vtype: u64, vl: u64, vstart: u64, frm: u8) {
        let insn = decoded(raw);
        let mut cpu = cpu(vtype, vl, vstart, frm);
        let before = architectural_state(&cpu);
        assert_eq!(
            cpu.execute_insn(&insn, 0x1000),
            Err(Trap::illegal(raw)),
            "reserved encoding {raw:08x} ({:?}) must trap",
            insn.op
        );
        assert_eq!(
            architectural_state(&cpu),
            before,
            "reserved encoding {raw:08x} modified state before trapping"
        );
    }

    fn assert_legal(raw: u32, vtype: u64, vl: u64, vstart: u64, frm: u8) {
        let insn = decoded(raw);
        let mut cpu = cpu(vtype, vl, vstart, frm);
        assert_eq!(
            cpu.execute_insn(&insn, 0x1000),
            Ok(RiscVExit::Continue),
            "legal encoding {raw:08x} ({:?}) trapped",
            insn.op
        );
    }

    #[test]
    fn mask_prefix_constraints_are_checked_before_execution() {
        for selector in [0b00001, 0b00010, 0b00011] {
            // Unmasked vd == vs2 is reserved.
            assert_illegal(op_v(0b010100, 1, 2, selector, 0b010, 2), E8_M1, 8, 0, 0);
            // A masked prefix instruction cannot write v0.
            assert_illegal(op_v(0b010100, 0, 2, selector, 0b010, 0), E8_M1, 8, 0, 0);
            // Prefix operations are not restartable.
            assert_illegal(op_v(0b010100, 1, 2, selector, 0b010, 1), E8_M1, 8, 1, 0);
            assert_legal(op_v(0b010100, 1, 2, selector, 0b010, 1), E8_M1, 8, 0, 0);
            assert_legal(op_v(0b010100, 0, 2, selector, 0b010, 1), E8_M1, 8, 0, 0);
        }
    }

    #[test]
    fn carry_and_borrow_require_vm_zero_and_nonzero_destination() {
        let forms = [
            (0b010000, 0b000), // vadc.vvm
            (0b010000, 0b100), // vadc.vxm
            (0b010000, 0b011), // vadc.vim
            (0b010010, 0b000), // vsbc.vvm
            (0b010010, 0b100), // vsbc.vxm
        ];
        for (funct6, funct3) in forms {
            assert_illegal(op_v(funct6, 1, 2, 3, funct3, 1), E8_M1, 8, 0, 0);
            assert_illegal(op_v(funct6, 0, 2, 3, funct3, 0), E8_M1, 8, 0, 0);
            assert_legal(op_v(funct6, 0, 2, 3, funct3, 1), E8_M1, 8, 0, 0);
        }
    }

    #[test]
    fn slide_up_rejects_overlap_and_misaligned_groups_but_down_allows_overlap() {
        const E8_M2: u64 = 0x01;
        let up_forms = [
            (0b001110, 0b100), // vslideup.vx
            (0b001110, 0b011), // vslideup.vi
            (0b001110, 0b110), // vslide1up.vx
            (0b001110, 0b101), // vfslide1up.vf
        ];
        for (funct6, funct3) in up_forms {
            assert_illegal(op_v(funct6, 1, 2, 3, funct3, 2), E8_M2, 4, 0, 0);
            assert_illegal(op_v(funct6, 1, 4, 3, funct3, 1), E8_M2, 4, 0, 0);
            assert_illegal(op_v(funct6, 1, 3, 3, funct3, 4), E8_M2, 4, 0, 0);
            assert_legal(op_v(funct6, 1, 4, 3, funct3, 2), E8_M2, 4, 0, 0);
        }

        // Downward slides explicitly permit source/destination overlap.
        for funct3 in [0b100, 0b011, 0b110, 0b101] {
            assert_legal(op_v(0b001111, 1, 2, 3, funct3, 2), E8_M2, 4, 0, 0);
        }
    }

    fn narrowing_encodings(vd: u32, vs2: u32, vector_shift: u32) -> Vec<u32> {
        let mut encodings = Vec::new();
        for funct6 in [0b101100, 0b101101, 0b101110, 0b101111] {
            encodings.push(op_v(funct6, 1, vs2, vector_shift, 0b000, vd));
            encodings.push(op_v(funct6, 1, vs2, 5, 0b100, vd));
            encodings.push(op_v(funct6, 1, vs2, 3, 0b011, vd));
        }
        for selector in 0b10000..=0b10111 {
            encodings.push(op_v(0b010010, 1, vs2, selector, 0b001, vd));
        }
        encodings
    }

    #[test]
    fn narrowing_overlap_uses_exact_rational_emul() {
        // LMUL >= 1: same-lowest-register overlap is legal, upper-part overlap
        // is reserved, and a disjoint aligned group is legal.
        for (vtype, same, partial, source, disjoint, vector_shift) in [
            (E32_M1, 2, 3, 2, 4, 6),
            (0x11, 4, 6, 4, 0, 16), // e32,m2: vd=6 overlaps upper half of v4-v7
            (0x12, 8, 12, 8, 0, 16), // e32,m4: vd=12 overlaps upper half of v8-v15
        ] {
            for raw in narrowing_encodings(same, source, vector_shift) {
                assert_legal(raw, vtype, 1, 0, 0);
            }
            for raw in narrowing_encodings(partial, source, vector_shift) {
                assert_illegal(raw, vtype, 1, 0, 0);
            }
            for raw in narrowing_encodings(disjoint, source, vector_shift) {
                assert_legal(raw, vtype, 1, 0, 0);
            }
        }

        // m8 would require an illegal wide source EMUL of 16 registers.
        for raw in narrowing_encodings(0, 0, 16) {
            assert_illegal(raw, 0x13, 1, 0, 0);
        }

        // Fractional LMULs occupy one named register. Odd register numbers are
        // valid, and widening mf8/mf4/mf2 does not invent a two-register group.
        for vtype in [0x15, 0x16, 0x17] {
            for raw in narrowing_encodings(1, 1, 4) {
                assert_legal(raw, vtype, 1, 0, 0);
            }
            for raw in narrowing_encodings(1, 3, 4) {
                assert_legal(raw, vtype, 1, 0, 0);
            }
        }
    }

    #[test]
    fn narrowing_rejects_misaligned_groups_and_reserved_lmul() {
        // e32,m2 has a two-register destination and a four-register wide source.
        for raw in narrowing_encodings(1, 4, 16) {
            assert_illegal(raw, 0x11, 1, 0, 0);
        }
        for raw in narrowing_encodings(0, 2, 16) {
            assert_illegal(raw, 0x11, 1, 0, 0);
        }
        // vlmul=100 is reserved even when injected directly through the test API.
        for raw in narrowing_encodings(0, 0, 4) {
            assert_illegal(raw, 0x14, 1, 0, 0);
        }
    }

    fn decoded_vector_fp_encodings() -> Vec<Insn> {
        let isa = Isa::rv64gc();
        let mut encodings = Vec::new();
        for funct6 in 0..64 {
            for funct3 in [0b001, 0b101] {
                for src in 0..32 {
                    let raw = op_v(funct6, 1, 2, src, funct3, 1);
                    let insn = decode(raw, Xlen::Rv64, &isa);
                    if insn.op != Op::Illegal {
                        encodings.push(insn);
                    }
                }
            }
        }
        // vfmv.s.f uses vs2=0 as an additional encoding constraint.
        encodings.push(decoded(op_v(0b010000, 1, 0, 3, 0b101, 1)));
        encodings
    }

    #[test]
    fn every_decoded_opfvv_and_opfvf_encoding_validates_frm() {
        let encodings = decoded_vector_fp_encodings();
        assert!(
            encodings.len() > 100,
            "decoder enumeration unexpectedly found too few FP encodings"
        );

        for insn in encodings {
            assert!(is_vector_fp_encoding(&insn), "missed {:?}", insn.op);
            for frm in 0..=4 {
                let cpu = cpu(E32_M1, 4, 0, frm);
                assert_eq!(
                    validate(&cpu, &insn, true),
                    Ok(()),
                    "legal frm={frm} rejected for {:?} ({:08x})",
                    insn.op,
                    insn.raw
                );
            }
            for frm in 5..=7 {
                let cpu = cpu(E32_M1, 0, 8, frm);
                assert_eq!(
                    validate(&cpu, &insn, true),
                    Err(Trap::illegal(insn.raw)),
                    "reserved frm={frm} accepted for {:?} ({:08x})",
                    insn.op,
                    insn.raw
                );
            }
        }

        let vector_load = decoded((1 << 25) | (10 << 15) | (6 << 12) | (1 << 7) | 0x07);
        assert!(!is_vector_fp_encoding(&vector_load));
    }

    #[test]
    fn reserved_frm_traps_before_vl_and_vstart_short_circuits() {
        let representatives = [
            op_v(0b000000, 1, 2, 3, 0b001, 1),       // vfadd.vv
            op_v(0b000100, 1, 2, 3, 0b001, 1),       // vfmin.vv (exact)
            op_v(0b001000, 1, 2, 3, 0b001, 1),       // vfsgnj.vv (exact)
            op_v(0b011000, 1, 2, 3, 0b001, 1),       // vmfeq.vv
            op_v(0b010011, 1, 2, 0b10000, 0b001, 1), // vfclass.v
            op_v(0b010000, 1, 2, 0, 0b001, 1),       // vfmv.f.s
            op_v(0b010000, 1, 0, 3, 0b101, 1),       // vfmv.s.f
            op_v(0b010011, 1, 2, 0b00100, 0b001, 1), // vfrsqrt7.v
            op_v(0b010010, 1, 2, 0b00110, 0b001, 1), // vfcvt.rtz.xu.f.v
            op_v(0b001110, 1, 2, 3, 0b101, 1),       // vfslide1up.vf
            op_v(0b000001, 1, 2, 3, 0b001, 1),       // vfredusum.vs
            op_v(0b110000, 1, 2, 3, 0b001, 1),       // vfwadd.vv
        ];

        for raw in representatives {
            for frm in 5..=7 {
                assert_illegal(raw, E32_M1, 0, 0, frm);
                assert_illegal(raw, E32_M1, 4, 4, frm);
            }
        }
    }
}
