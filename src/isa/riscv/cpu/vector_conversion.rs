//! Mixed-width RVV conversion semantics.
//!
//! This module owns the narrowing floating-point/integer conversion group so
//! its operand-width admission and data conversion remain one audited unit.

use crate::isa::riscv::float::{self, RoundingMode};

use super::{Insn, Op, RiscVCpu, Trap, fmt_eb, sext_sew};

impl RiscVCpu {
    pub(super) fn exec_vector_narrow_conversion(
        &mut self,
        insn: &Insn,
        vm: bool,
    ) -> Result<(), Trap> {
        // Narrowing conversions read a 2*SEW source and write an SEW result.
        // At SEW=8, Zvfh defines only the FP16-to-integer8 variants; any form
        // producing FP8 remains reserved. SEW=64 would require an unsupported
        // 128-bit source and is reserved for every variant.
        let eb = self.sew_bytes();
        let to_integer = matches!(
            insn.op,
            Op::VfncvtXuF | Op::VfncvtXF | Op::VfncvtRtzXuF | Op::VfncvtRtzXF
        );
        if eb > 4 || (eb == 1 && !to_integer) {
            return Err(Trap::illegal(insn.raw));
        }

        let web = eb * 2;
        let mask = Self::sew_mask(eb);
        let frm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
        let mut flags = 0u32;
        for element in self.vstart as usize..self.vl as usize {
            if !vm && !self.vmask_bit(element) {
                continue;
            }
            let wide = self.velem(insn.rs2, element, web);
            let result = match insn.op {
                Op::VfncvtXuF | Op::VfncvtXF | Op::VfncvtRtzXuF | Op::VfncvtRtzXF => {
                    let signed = matches!(insn.op, Op::VfncvtXF | Op::VfncvtRtzXF);
                    let rounding = if matches!(insn.op, Op::VfncvtRtzXuF | Op::VfncvtRtzXF) {
                        RoundingMode::Rtz
                    } else {
                        frm
                    };
                    match web {
                        2 => float::ftoi(
                            float::h_widen(wide as u16),
                            signed,
                            (eb * 8) as u32,
                            rounding,
                            &mut flags,
                        ),
                        4 => float::ftoi(
                            f32::from_bits(wide as u32),
                            signed,
                            (eb * 8) as u32,
                            rounding,
                            &mut flags,
                        ),
                        _ => float::ftoi(
                            f64::from_bits(wide),
                            signed,
                            (eb * 8) as u32,
                            rounding,
                            &mut flags,
                        ),
                    }
                }
                Op::VfncvtFXu | Op::VfncvtFX => {
                    let value: i128 = if insn.op == Op::VfncvtFX {
                        sext_sew(wide, web) as i128
                    } else {
                        wide as i128
                    };
                    float::itof_fmt(fmt_eb(eb), value, frm, &mut flags)
                }
                Op::VfncvtRodFF => {
                    // Round to odd by truncating, then force the result LSB for
                    // an inexact conversion.
                    let mut conversion_flags = 0u32;
                    let narrowed = float::fcvt_round(
                        fmt_eb(web),
                        fmt_eb(eb),
                        wide,
                        RoundingMode::Rtz,
                        &mut conversion_flags,
                    );
                    flags |= conversion_flags;
                    if conversion_flags & 1 != 0 {
                        narrowed | 1
                    } else {
                        narrowed
                    }
                }
                Op::VfncvtFF => float::fcvt_round(fmt_eb(web), fmt_eb(eb), wide, frm, &mut flags),
                _ => return Err(Trap::illegal(insn.raw)),
            };
            self.set_velem(insn.rd, element, eb, result & mask);
        }
        self.accrue(flags);
        Ok(())
    }
}
