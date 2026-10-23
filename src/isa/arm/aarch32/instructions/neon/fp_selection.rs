//! Legacy Advanced SIMD selection/comparison under StandardFPSCRValue.
use super::*;
use crate::smir::interpret::SmirInterpreter;

impl<'a, M: ArmMemory> Executor<'a, M> {
    pub(crate) fn exec_neon_fp_minmax(&mut self, insn: &DecodedInsn) -> ExecResult {
        self.with_standard_fp_controls(|executor| executor.exec_neon_fp_minmax_inner(insn))
    }
    fn exec_neon_fp_minmax_inner(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if (insn.raw >> 24) != 0xF2
            || ((insn.raw >> 23) & 1) != 0
            || ((insn.raw >> 8) & 0xF) != 0b1111
            || ((insn.raw >> 4) & 1) != 0
        {
            return ExecResult::Undefined;
        }

        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let n_bit = ((insn.raw >> 7) & 1) as u8;
        let vn = ((insn.raw >> 16) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let q = ((insn.raw >> 6) & 1) != 0;
        let regs = if q { 2 } else { 1 };

        let d = (d_bit << 4) | vd;
        let n = (n_bit << 4) | vn;
        let m = (m_bit << 4) | vm;
        if q && ((d | n | m) & 1) != 0 {
            return ExecResult::Undefined;
        }
        if d + regs > 32 || n + regs > 32 || m + regs > 32 {
            return ExecResult::Undefined;
        }

        let size = if ((insn.raw >> 20) & 1) == 0 {
            NeonSize::S32
        } else {
            NeonSize::H16
        };
        let ebytes = (size.bits() / 8) as u8;

        for reg in 0..regs {
            let n_elements = self.neon_read_vector_elements_u64(n + reg, 1, ebytes);
            let m_elements = self.neon_read_vector_elements_u64(m + reg, 1, ebytes);
            let mut out = Vec::with_capacity(n_elements.len());
            for (n_elem, m_elem) in n_elements.into_iter().zip(m_elements.into_iter()) {
                let fpscr = &mut self.cpu.vfp.fpscr;
                let result = match size {
                    NeonSize::S32 => {
                        let n_val = f32::from_bits(n_elem as u32);
                        let m_val = f32::from_bits(m_elem as u32);
                        (match insn.mnemonic {
                            Mnemonic::VMAX => Self::neon_fpmax_f32_bits(n_val, m_val, fpscr),
                            Mnemonic::VMIN => Self::neon_fpmin_f32_bits(n_val, m_val, fpscr),
                            _ => return ExecResult::Undefined,
                        }) as u64
                    }
                    NeonSize::H16 => {
                        let n_val = vcvt_f32_f16_bits(n_elem as u16);
                        let m_val = vcvt_f32_f16_bits(m_elem as u16);
                        match insn.mnemonic {
                            Mnemonic::VMAX => vcvt_f16_bits_f32(
                                f32::from_bits(Self::neon_fpmax_f32_bits(n_val, m_val, fpscr)),
                                fpscr,
                            ) as u64,
                            Mnemonic::VMIN => vcvt_f16_bits_f32(
                                f32::from_bits(Self::neon_fpmin_f32_bits(n_val, m_val, fpscr)),
                                fpscr,
                            ) as u64,
                            _ => return ExecResult::Undefined,
                        }
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        ExecResult::Continue
    }
    pub(crate) fn exec_neon_fp_compare(&mut self, insn: &DecodedInsn) -> ExecResult {
        self.with_standard_fp_controls(|executor| executor.exec_neon_fp_compare_inner(insn))
    }
    fn exec_neon_fp_compare_inner(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if (insn.raw >> 25) != 0b1111001
            || ((insn.raw >> 23) & 1) != 0
            || ((insn.raw >> 8) & 0xF) != 0b1110
        {
            return ExecResult::Undefined;
        }

        let bit24 = (insn.raw >> 24) & 1;
        let bit21 = (insn.raw >> 21) & 1;
        let bit20 = (insn.raw >> 20) & 1;
        let absolute = ((insn.raw >> 4) & 1) != 0;
        match (insn.mnemonic, absolute, bit24, bit21, bit20) {
            (Mnemonic::VCEQ, false, 0, 0, 0 | 1)
            | (Mnemonic::VCGE, false, 1, 0, 0 | 1)
            | (Mnemonic::VCGT, false, 1, 1, 0 | 1)
            | (Mnemonic::VACGE, true, 1, 0, 0 | 1)
            | (Mnemonic::VACGT, true, 1, 1, 0 | 1) => {}
            _ => return ExecResult::Undefined,
        }

        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let n_bit = ((insn.raw >> 7) & 1) as u8;
        let vn = ((insn.raw >> 16) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let q = ((insn.raw >> 6) & 1) != 0;
        let regs = if q { 2 } else { 1 };

        let d = (d_bit << 4) | vd;
        let n = (n_bit << 4) | vn;
        let m = (m_bit << 4) | vm;
        if q && ((d | n | m) & 1) != 0 {
            return ExecResult::Undefined;
        }
        if d + regs > 32 || n + regs > 32 || m + regs > 32 {
            return ExecResult::Undefined;
        }

        let size = if bit20 == 0 {
            NeonSize::S32
        } else {
            NeonSize::H16
        };
        let ebytes = (size.bits() / 8) as u8;
        let true_mask = if size == NeonSize::S32 {
            u64::from(u32::MAX)
        } else {
            u64::from(u16::MAX)
        };

        for reg in 0..regs {
            let n_elements = self.neon_read_vector_elements_u64(n + reg, 1, ebytes);
            let m_elements = self.neon_read_vector_elements_u64(m + reg, 1, ebytes);
            let mut out = Vec::with_capacity(n_elements.len());
            for (n_elem, m_elem) in n_elements.into_iter().zip(m_elements.into_iter()) {
                let mut lhs = match size {
                    NeonSize::S32 => f32::from_bits(n_elem as u32),
                    NeonSize::H16 => vcvt_f32_f16_bits(n_elem as u16),
                    _ => return ExecResult::Undefined,
                };
                let mut rhs = match size {
                    NeonSize::S32 => f32::from_bits(m_elem as u32),
                    NeonSize::H16 => vcvt_f32_f16_bits(m_elem as u16),
                    _ => return ExecResult::Undefined,
                };
                if absolute {
                    lhs = lhs.abs();
                    rhs = rhs.abs();
                }
                let (nzcv, status) = SmirInterpreter::arm_eval_compare(
                    lhs.to_bits() as u64,
                    rhs.to_bits() as u64,
                    false,
                    insn.mnemonic != Mnemonic::VCEQ,
                    self.cpu.vfp.fpscr.bits(),
                );
                self.cpu.vfp.fpscr = Fpscr::from_bits(self.cpu.vfp.fpscr.bits() | status);
                let condition = match insn.mnemonic {
                    Mnemonic::VCEQ => nzcv == 6 << 28,
                    Mnemonic::VCGT | Mnemonic::VACGT => nzcv == 2 << 28,
                    Mnemonic::VCGE | Mnemonic::VACGE => nzcv == 2 << 28 || nzcv == 6 << 28,
                    _ => return ExecResult::Undefined,
                };
                out.push(if condition { true_mask } else { 0 });
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        ExecResult::Continue
    }
    pub(crate) fn neon_fpmax_f32_bits(a: f32, b: f32, fpscr: &mut Fpscr) -> u32 {
        let (bits, status) = SmirInterpreter::arm_eval_minmax(
            a.to_bits() as u64,
            b.to_bits() as u64,
            false,
            true,
            false,
            fpscr.bits(),
        );
        *fpscr = Fpscr::from_bits(fpscr.bits() | status);
        bits as u32
    }
    pub(crate) fn neon_fpmin_f32_bits(a: f32, b: f32, fpscr: &mut Fpscr) -> u32 {
        let (bits, status) = SmirInterpreter::arm_eval_minmax(
            a.to_bits() as u64,
            b.to_bits() as u64,
            false,
            false,
            false,
            fpscr.bits(),
        );
        *fpscr = Fpscr::from_bits(fpscr.bits() | status);
        bits as u32
    }
    pub(crate) fn exec_neon_fp_absdiff(&mut self, insn: &DecodedInsn) -> ExecResult {
        self.with_standard_fp_controls(|executor| executor.exec_neon_fp_absdiff_inner(insn))
    }
    fn exec_neon_fp_absdiff_inner(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if (insn.raw >> 24) != 0xF3
            || ((insn.raw >> 23) & 1) != 0
            || ((insn.raw >> 21) & 1) != 1
            || ((insn.raw >> 8) & 0xF) != 0b1101
            || ((insn.raw >> 4) & 1) != 0
        {
            return ExecResult::Undefined;
        }

        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let n_bit = ((insn.raw >> 7) & 1) as u8;
        let vn = ((insn.raw >> 16) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let q = ((insn.raw >> 6) & 1) != 0;
        let regs = if q { 2 } else { 1 };

        let d = (d_bit << 4) | vd;
        let n = (n_bit << 4) | vn;
        let m = (m_bit << 4) | vm;
        if q && ((d | n | m) & 1) != 0 {
            return ExecResult::Undefined;
        }
        if d + regs > 32 || n + regs > 32 || m + regs > 32 {
            return ExecResult::Undefined;
        }

        let size = if ((insn.raw >> 20) & 1) == 0 {
            NeonSize::S32
        } else {
            NeonSize::H16
        };
        let ebytes = (size.bits() / 8) as u8;

        for reg in 0..regs {
            let n_elements = self.neon_read_vector_elements_u64(n + reg, 1, ebytes);
            let m_elements = self.neon_read_vector_elements_u64(m + reg, 1, ebytes);
            let mut out = Vec::with_capacity(n_elements.len());
            for (n_elem, m_elem) in n_elements.into_iter().zip(m_elements.into_iter()) {
                let result = match size {
                    NeonSize::S32 => {
                        let n_val = f32::from_bits(n_elem as u32);
                        let m_val = f32::from_bits(m_elem as u32);
                        vsub_f32(n_val, m_val, &mut self.cpu.vfp.fpscr)
                            .abs()
                            .to_bits() as u64
                    }
                    NeonSize::H16 => {
                        let n_val = vcvt_f32_f16_bits(n_elem as u16);
                        let m_val = vcvt_f32_f16_bits(m_elem as u16);
                        let fpscr = &mut self.cpu.vfp.fpscr;
                        let difference = vsub_f32(n_val, m_val, fpscr);
                        vcvt_f16_bits_f32(difference.abs(), fpscr) as u64
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        ExecResult::Continue
    }
}
