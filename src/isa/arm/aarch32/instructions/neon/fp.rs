//! fp.rs

use crate::isa::arm::ExecutionState;
use crate::isa::arm::aarch32::cpu::{
    ArmMemory, Armv7Cpu, MemoryError, ProcessorMode, Psr, add_with_carry, compute_n_flag,
    compute_z_flag, condition_passed, expand_imm_c, shift_c, sign_extend,
};
use crate::isa::arm::aarch32::instructions::neon::*;
use crate::isa::arm::aarch32::instructions::*;
use crate::isa::arm::aarch32::vfp::{
    Fpscr, NeonSize, RoundingMode, vabs_f16_bits, vabs_f32, vabs_f64, vadd_f16_bits, vadd_f32,
    vadd_f64, vadd_i, vand, vbic, vcls_i, vclz_i, vcmp_f16_bits_with_exception,
    vcmp_f32_with_exception, vcmp_f64_with_exception, vcnt_i8, vcvt_f16_bits_f32,
    vcvt_f32_f16_bits, vcvt_f32_f64, vcvt_f32_s32, vcvt_f32_s32_fixed, vcvt_f32_u32,
    vcvt_f32_u32_fixed, vcvt_f64_f32, vcvt_f64_s32, vcvt_f64_s32_fixed, vcvt_f64_u32,
    vcvt_f64_u32_fixed, vcvt_s32_f32, vcvt_s32_f32_fixed, vcvt_s32_f32_round, vcvt_s32_f64,
    vcvt_s32_f64_fixed, vcvt_s32_f64_round, vcvt_u32_f32, vcvt_u32_f32_fixed, vcvt_u32_f32_round,
    vcvt_u32_f64, vcvt_u32_f64_fixed, vcvt_u32_f64_round, vcvtr_s32_f32, vcvtr_s32_f64,
    vcvtr_u32_f32, vcvtr_u32_f64, vdiv_f16_bits, vdiv_f32, vdiv_f64, veor, vfma_f16_bits, vfma_f32,
    vfma_f64, vfms_f16_bits, vfms_f32, vfms_f64, vfnma_f16_bits, vfnma_f32, vfnma_f64,
    vfnms_f16_bits, vfnms_f32, vfnms_f64, vfp_expand_imm_f16, vfp_expand_imm_f32,
    vfp_expand_imm_f64, vmaxnm_f16_bits, vmaxnm_f32, vmaxnm_f64, vminnm_f16_bits, vminnm_f32,
    vminnm_f64, vmla_f16_bits, vmla_f32, vmla_f64, vmls_f16_bits, vmls_f32, vmls_f64,
    vmul_f16_bits, vmul_f32, vmul_f64, vmvn, vneg_f16_bits, vneg_f32, vneg_f64, vnmla_f16_bits,
    vnmla_f32, vnmla_f64, vnmls_f16_bits, vnmls_f32, vnmls_f64, vnmul_f16_bits, vnmul_f32,
    vnmul_f64, vorn, vorr, vrev, vrint_f16_bits, vrint_f32, vrint_f64, vsqrt_f16_bits, vsqrt_f32,
    vsqrt_f64, vsub_f16_bits, vsub_f32, vsub_f64, vsub_i,
};
use crate::isa::arm::decoder::{Condition, DecodeError, DecodedInsn, Mnemonic, ShiftType};

impl<'a, M: ArmMemory> Executor<'a, M> {
    pub(crate) fn is_neon_fp_add_sub_shape(raw: u32) -> bool {
        (raw >> 25) == 0b1111001
            && ((raw >> 24) & 1) == 0
            && ((raw >> 23) & 1) == 0
            && ((raw >> 8) & 0xF) == 0b1101
            && ((raw >> 4) & 1) == 0
    }

    pub(crate) fn exec_neon_fp_add_sub(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !Self::is_neon_fp_add_sub_shape(insn.raw) {
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
                        u64::from(
                            match insn.mnemonic {
                                Mnemonic::VADD => vadd_f32(n_val, m_val, fpscr),
                                Mnemonic::VSUB => vsub_f32(n_val, m_val, fpscr),
                                _ => return ExecResult::Undefined,
                            }
                            .to_bits(),
                        )
                    }
                    NeonSize::H16 => {
                        let n_val = n_elem as u16;
                        let m_val = m_elem as u16;
                        u64::from(match insn.mnemonic {
                            Mnemonic::VADD => vadd_f16_bits(n_val, m_val, fpscr),
                            Mnemonic::VSUB => vsub_f16_bits(n_val, m_val, fpscr),
                            _ => return ExecResult::Undefined,
                        })
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        ExecResult::Continue
    }

    pub(crate) fn is_neon_fp_pairwise_shape(raw: u32) -> bool {
        (raw >> 25) == 0b1111001
            && ((raw >> 24) & 1) == 1
            && ((raw >> 23) & 1) == 0
            && matches!(
                ((raw >> 8) & 0xF, (raw >> 21) & 1),
                (0b1101, 0) | (0b1111, 0 | 1)
            )
            && ((raw >> 4) & 1) == 0
    }

    pub(crate) fn exec_neon_fp_pairwise(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !matches!(
            insn.mnemonic,
            Mnemonic::VPADD | Mnemonic::VPMAX | Mnemonic::VPMIN
        ) || !Self::is_neon_fp_pairwise_shape(insn.raw)
        {
            return ExecResult::Undefined;
        }
        if ((insn.raw >> 6) & 1) != 0 {
            return ExecResult::Undefined;
        }

        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let n_bit = ((insn.raw >> 7) & 1) as u8;
        let vn = ((insn.raw >> 16) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let d = (d_bit << 4) | vd;
        let n = (n_bit << 4) | vn;
        let m = (m_bit << 4) | vm;
        if d >= 32 || n >= 32 || m >= 32 {
            return ExecResult::Undefined;
        }

        let size = if ((insn.raw >> 20) & 1) == 0 {
            NeonSize::S32
        } else {
            NeonSize::H16
        };
        let ebytes = (size.bits() / 8) as u8;
        let n_elements = self.neon_read_vector_elements_u64(n, 1, ebytes);
        let m_elements = self.neon_read_vector_elements_u64(m, 1, ebytes);
        let fpscr = &mut self.cpu.vfp.fpscr;
        let mut out = Vec::with_capacity(n_elements.len());
        for elements in [&n_elements, &m_elements] {
            for pair in elements.chunks_exact(2) {
                let result = match size {
                    NeonSize::S32 => {
                        let lhs = f32::from_bits(pair[0] as u32);
                        let rhs = f32::from_bits(pair[1] as u32);
                        u64::from(match insn.mnemonic {
                            Mnemonic::VPADD => vadd_f32(lhs, rhs, fpscr).to_bits(),
                            Mnemonic::VPMAX => Self::neon_fpmax_f32_bits(lhs, rhs),
                            Mnemonic::VPMIN => Self::neon_fpmin_f32_bits(lhs, rhs),
                            _ => return ExecResult::Undefined,
                        })
                    }
                    NeonSize::H16 => {
                        let lhs = pair[0] as u16;
                        let rhs = pair[1] as u16;
                        u64::from(match insn.mnemonic {
                            Mnemonic::VPADD => vadd_f16_bits(lhs, rhs, fpscr),
                            Mnemonic::VPMAX => {
                                let lhs_f = vcvt_f32_f16_bits(lhs);
                                let rhs_f = vcvt_f32_f16_bits(rhs);
                                vcvt_f16_bits_f32(
                                    f32::from_bits(Self::neon_fpmax_f32_bits(lhs_f, rhs_f)),
                                    fpscr,
                                )
                            }
                            Mnemonic::VPMIN => {
                                let lhs_f = vcvt_f32_f16_bits(lhs);
                                let rhs_f = vcvt_f32_f16_bits(rhs);
                                vcvt_f16_bits_f32(
                                    f32::from_bits(Self::neon_fpmin_f32_bits(lhs_f, rhs_f)),
                                    fpscr,
                                )
                            }
                            _ => return ExecResult::Undefined,
                        })
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
        }

        self.neon_write_vector_elements_u64(d, 1, ebytes, &out);
        ExecResult::Continue
    }

    pub(crate) fn is_neon_vrint_shape(raw: u32) -> bool {
        (raw >> 24) == 0xF3
            && ((raw >> 23) & 1) == 1
            && ((raw >> 21) & 1) == 1
            && ((raw >> 20) & 1) == 1
            && ((raw >> 16) & 0x3) == 0b10
            && ((raw >> 10) & 0x3) == 0b01
            && matches!(
                (raw >> 7) & 0x7,
                0b000 | 0b001 | 0b010 | 0b011 | 0b101 | 0b111
            )
            && ((raw >> 4) & 1) == 0
    }

    pub(crate) fn exec_neon_vrint(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !Self::is_neon_vrint_shape(insn.raw) {
            return ExecResult::Undefined;
        }

        let size = match (insn.raw >> 18) & 0x3 {
            0b01 => NeonSize::H16,
            0b10 => NeonSize::S32,
            _ => return ExecResult::Undefined,
        };
        let Some((mode, exact)) = self.vrint_rounding(insn.mnemonic) else {
            return ExecResult::Undefined;
        };

        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let q = ((insn.raw >> 6) & 1) != 0;
        let regs = if q { 2 } else { 1 };
        let d = (d_bit << 4) | vd;
        let m = (m_bit << 4) | vm;
        if q && ((d | m) & 1) != 0 {
            return ExecResult::Undefined;
        }
        if d + regs > 32 || m + regs > 32 {
            return ExecResult::Undefined;
        }

        let ebytes = (size.bits() / 8) as u8;
        for reg in 0..regs {
            let elements = self.neon_read_vector_elements_u64(m + reg, 1, ebytes);
            let mut out = Vec::with_capacity(elements.len());
            for elem in elements {
                let result = match size {
                    NeonSize::H16 => u64::from(vrint_f16_bits(
                        elem as u16,
                        mode,
                        exact,
                        &mut self.cpu.vfp.fpscr,
                    )),
                    NeonSize::S32 => u64::from(
                        vrint_f32(
                            f32::from_bits(elem as u32),
                            mode,
                            exact,
                            &mut self.cpu.vfp.fpscr,
                        )
                        .to_bits(),
                    ),
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        ExecResult::Continue
    }

    pub(crate) fn is_neon_fp_multiply_shape(raw: u32) -> bool {
        if (raw >> 25) != 0b1111001
            || ((raw >> 23) & 1) != 0
            || ((raw >> 8) & 0xF) != 0b1101
            || ((raw >> 4) & 1) != 1
        {
            return false;
        }

        matches!(
            (((raw >> 24) & 1) != 0, ((raw >> 21) & 1) != 0),
            (true, false) | (false, false) | (false, true)
        )
    }

    pub(crate) fn is_neon_fp_multiply_scalar_shape(raw: u32) -> bool {
        (raw >> 25) == 0b1111001
            && ((raw >> 23) & 1) == 1
            && matches!((raw >> 20) & 0x3, 0b01 | 0b10)
            && ((raw >> 6) & 1) == 1
            && ((raw >> 4) & 1) == 0
            && matches!((raw >> 8) & 0xF, 0b0001 | 0b0101 | 0b1001)
    }

    pub(crate) fn is_neon_fp_fma_shape(raw: u32) -> bool {
        (raw >> 25) == 0b1111001
            && ((raw >> 24) & 1) == 0
            && ((raw >> 23) & 1) == 0
            && ((raw >> 20) & 1) == 0
            && ((raw >> 8) & 0xF) == 0b1100
            && ((raw >> 4) & 1) == 1
    }

    pub(crate) fn is_neon_fp16_fused_multiply_long_shape(raw: u32) -> bool {
        ((raw >> 24) == 0xFC
            && ((raw >> 21) & 1) == 1
            && ((raw >> 20) & 1) == 0
            && ((raw >> 8) & 0xF) == 0b1000
            && ((raw >> 4) & 1) == 1)
            || ((raw >> 24) == 0xFE
                && ((raw >> 23) & 1) == 0
                && ((raw >> 21) & 1) == 0
                && ((raw >> 8) & 0xF) == 0b1000
                && ((raw >> 4) & 1) == 1)
    }

    pub(crate) fn exec_neon_fp_multiply(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        let scalar = Self::is_neon_fp_multiply_scalar_shape(insn.raw);
        if !Self::is_neon_fp_multiply_shape(insn.raw)
            && !scalar
            && !Self::is_neon_fp_fma_shape(insn.raw)
        {
            return ExecResult::Undefined;
        }

        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let n_bit = ((insn.raw >> 7) & 1) as u8;
        let vn = ((insn.raw >> 16) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let q = if scalar {
            ((insn.raw >> 24) & 1) != 0
        } else {
            ((insn.raw >> 6) & 1) != 0
        };
        let regs = if q { 2 } else { 1 };

        let d = (d_bit << 4) | vd;
        let n = (n_bit << 4) | vn;
        let m = (m_bit << 4) | vm;
        if q && ((d | n | if scalar { 0 } else { m }) & 1) != 0 {
            return ExecResult::Undefined;
        }
        if d + regs > 32 || n + regs > 32 || (!scalar && m + regs > 32) {
            return ExecResult::Undefined;
        }
        let size = if scalar {
            match (insn.raw >> 20) & 0x3 {
                0b01 => NeonSize::H16,
                0b10 => NeonSize::S32,
                _ => return ExecResult::Undefined,
            }
        } else if !Self::is_neon_fp_fma_shape(insn.raw) && ((insn.raw >> 20) & 1) != 0 {
            NeonSize::H16
        } else {
            NeonSize::S32
        };
        let ebytes = (size.bits() / 8) as u8;
        let scalar_elem = if scalar {
            let (scalar_reg, scalar_index) = match size {
                NeonSize::H16 => (vm & 0x7, (m_bit << 1) | (vm >> 3)),
                NeonSize::S32 => (vm, m_bit),
                _ => return ExecResult::Undefined,
            };
            if scalar_reg >= 32 || scalar_index as usize >= size.elements_per_d() {
                return ExecResult::Undefined;
            }
            Some(self.neon_read_d_elem_u64(scalar_reg, scalar_index, ebytes))
        } else {
            None
        };

        for reg in 0..regs {
            let n_elements = self.neon_read_vector_elements_u64(n + reg, 1, ebytes);
            let m_elements = if let Some(elem) = scalar_elem {
                vec![elem; n_elements.len()]
            } else {
                self.neon_read_vector_elements_u64(m + reg, 1, ebytes)
            };
            let d_elements = if matches!(
                insn.mnemonic,
                Mnemonic::VMLA | Mnemonic::VMLS | Mnemonic::VFMA | Mnemonic::VFMS
            ) {
                self.neon_read_vector_elements_u64(d + reg, 1, ebytes)
            } else {
                vec![0; n_elements.len()]
            };
            let mut out = Vec::with_capacity(n_elements.len());
            for ((n_elem, m_elem), d_elem) in n_elements
                .into_iter()
                .zip(m_elements.into_iter())
                .zip(d_elements.into_iter())
            {
                let mut fpscr = self.cpu.vfp.fpscr;
                let result = match size {
                    NeonSize::S32 => {
                        let n_val = f32::from_bits(n_elem as u32);
                        let m_val = f32::from_bits(m_elem as u32);
                        u64::from(
                            match insn.mnemonic {
                                Mnemonic::VMUL => vmul_f32(n_val, m_val, &mut fpscr),
                                Mnemonic::VMLA => vmla_f32(
                                    f32::from_bits(d_elem as u32),
                                    n_val,
                                    m_val,
                                    &mut fpscr,
                                ),
                                Mnemonic::VMLS => vmls_f32(
                                    f32::from_bits(d_elem as u32),
                                    n_val,
                                    m_val,
                                    &mut fpscr,
                                ),
                                Mnemonic::VFMA => vfma_f32(
                                    f32::from_bits(d_elem as u32),
                                    n_val,
                                    m_val,
                                    &mut fpscr,
                                ),
                                Mnemonic::VFMS => vfms_f32(
                                    f32::from_bits(d_elem as u32),
                                    n_val,
                                    m_val,
                                    &mut fpscr,
                                ),
                                _ => return ExecResult::Undefined,
                            }
                            .to_bits(),
                        )
                    }
                    NeonSize::H16 => {
                        let n_val = n_elem as u16;
                        let m_val = m_elem as u16;
                        u64::from(match insn.mnemonic {
                            Mnemonic::VMUL => vmul_f16_bits(n_val, m_val, &mut fpscr),
                            Mnemonic::VMLA => {
                                vmla_f16_bits(d_elem as u16, n_val, m_val, &mut fpscr)
                            }
                            Mnemonic::VMLS => {
                                vmls_f16_bits(d_elem as u16, n_val, m_val, &mut fpscr)
                            }
                            _ => return ExecResult::Undefined,
                        })
                    }
                    _ => return ExecResult::Undefined,
                };
                self.cpu.vfp.fpscr = fpscr;
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        ExecResult::Continue
    }

    pub(crate) fn exec_neon_fp16_fused_multiply_long(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !Self::is_neon_fp16_fused_multiply_long_shape(insn.raw) {
            return ExecResult::Undefined;
        }

        let vector = (insn.raw >> 24) == 0xFC;
        let subtract = if vector {
            ((insn.raw >> 23) & 1) != 0
        } else {
            ((insn.raw >> 20) & 1) != 0
        };
        match (insn.mnemonic, subtract) {
            (Mnemonic::VFMAL, false) | (Mnemonic::VFMLS, true) => {}
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
        if q && (d & 1) != 0 {
            return ExecResult::Undefined;
        }
        if d + regs > 32 {
            return ExecResult::Undefined;
        }

        let n = if q {
            (n_bit << 4) | vn
        } else {
            (vn << 1) | n_bit
        };
        let m = if vector {
            if q {
                (m_bit << 4) | vm
            } else {
                (vm << 1) | m_bit
            }
        } else if q {
            vm & 0x7
        } else {
            ((vm & 0x7) << 1) | m_bit
        };
        if n >= 32 || m >= 32 {
            return ExecResult::Undefined;
        }

        let index = if !vector {
            Some(if q { (m_bit << 1) | (vm >> 3) } else { vm >> 3 })
        } else {
            None
        };
        let scalar = index.map(|lane| {
            if q {
                self.neon_read_d_elem_u64(m, lane, 2) as u16
            } else {
                ((self.cpu.vfp.read_s_bits(m) >> (lane * 16)) & 0xFFFF) as u16
            }
        });

        for reg in 0..regs {
            let operand1 = if q {
                self.cpu.vfp.read_d_bits(n)
            } else {
                u64::from(self.cpu.vfp.read_s_bits(n))
            };
            let operand2 = if scalar.is_some() {
                0
            } else if q {
                self.cpu.vfp.read_d_bits(m)
            } else {
                u64::from(self.cpu.vfp.read_s_bits(m))
            };
            let acc = self.cpu.vfp.read_d_bits(d + reg);
            let mut out = 0u64;
            for lane in 0..2 {
                let source_lane = if q { 2 * reg + lane } else { lane };
                let shift = source_lane * 16;
                let mut lhs = ((operand1 >> shift) & 0xFFFF) as u16;
                let rhs = scalar.unwrap_or_else(|| ((operand2 >> shift) & 0xFFFF) as u16);
                if subtract {
                    lhs ^= 0x8000;
                }
                let acc_lane = f32::from_bits(((acc >> (lane * 32)) & 0xFFFF_FFFF) as u32);
                let result = vcvt_f32_f16_bits(lhs).mul_add(vcvt_f32_f16_bits(rhs), acc_lane);
                out |= u64::from(result.to_bits()) << (lane * 32);
            }
            self.cpu.vfp.write_d_bits(d + reg, out);
        }

        ExecResult::Continue
    }

    pub(crate) fn neon_fp_recip_estimate_f32(bits: u32) -> u32 {
        let sign = bits >> 31;
        let exp = (bits >> 23) & 0xFF;
        let frac = bits & 0x7F_FFFF;
        if exp == 0xFF {
            return if frac != 0 {
                bits | 0x40_0000
            } else {
                sign << 31
            };
        }
        if exp == 0 && frac == 0 {
            return (sign << 31) | (0xFF << 23);
        }
        if exp == 0 && frac < 0x20_0000 {
            return (sign << 31) | (0xFF << 23);
        }

        let mut fraction: u64 = (frac as u64) << 29;
        let mut e = exp as i32;
        if e == 0 {
            if (fraction >> 51) & 1 == 0 {
                e = -1;
                fraction = (fraction << 2) & ((1u64 << 52) - 1);
            } else {
                fraction = (fraction << 1) & ((1u64 << 52) - 1);
            }
        }
        let scaled = 0x100 | ((fraction >> 44) & 0xFF) as u32;
        let estimate = Self::neon_recip_estimate(scaled);
        let mut result_exp = 253i32 - e;
        let mut out_frac: u64 = ((estimate & 0xFF) as u64) << 44;
        if result_exp == 0 {
            out_frac = (1u64 << 51) | (out_frac >> 1);
        } else if result_exp == -1 {
            out_frac = (1u64 << 50) | (out_frac >> 2);
            result_exp = 0;
        }
        (sign << 31) | (((result_exp as u32) & 0xFF) << 23) | ((out_frac >> 29) as u32 & 0x7F_FFFF)
    }

    pub(crate) fn neon_fp_rsqrt_estimate_f32(bits: u32) -> u32 {
        let sign = bits >> 31;
        let exp = (bits >> 23) & 0xFF;
        let frac = bits & 0x7F_FFFF;
        if exp == 0xFF && frac != 0 {
            return bits | 0x40_0000;
        }
        if exp == 0 && frac == 0 {
            return (sign << 31) | (0xFF << 23);
        }
        if sign == 1 {
            return 0x7FC0_0000;
        }
        if exp == 0xFF {
            return 0;
        }

        let mut fraction: u64 = (frac as u64) << 29;
        let mut e = exp as i32;
        if e == 0 {
            while (fraction >> 51) & 1 == 0 {
                fraction = (fraction << 1) & 0xF_FFFF_FFFF_FFFF;
                e -= 1;
            }
            fraction = (fraction << 1) & 0xF_FFFF_FFFF_FFFF;
        }
        let scaled = if e & 1 == 0 {
            0x100 | ((fraction >> 44) & 0xFF) as u32
        } else {
            0x80 | ((fraction >> 45) & 0x7F) as u32
        };
        let result_exp = (((380 - e) / 2) as u32) & 0xFF;
        let estimate = Self::neon_recip_sqrt_estimate(scaled);
        (sign << 31) | (result_exp << 23) | ((estimate & 0xFF) << 15)
    }

    pub(crate) fn exec_neon_fp_minmax(&mut self, insn: &DecodedInsn) -> ExecResult {
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
                let result = match size {
                    NeonSize::S32 => {
                        let n_val = f32::from_bits(n_elem as u32);
                        let m_val = f32::from_bits(m_elem as u32);
                        (match insn.mnemonic {
                            Mnemonic::VMAX => Self::neon_fpmax_f32_bits(n_val, m_val),
                            Mnemonic::VMIN => Self::neon_fpmin_f32_bits(n_val, m_val),
                            _ => return ExecResult::Undefined,
                        }) as u64
                    }
                    NeonSize::H16 => {
                        let n_val = vcvt_f32_f16_bits(n_elem as u16);
                        let m_val = vcvt_f32_f16_bits(m_elem as u16);
                        let mut fpscr = self.cpu.vfp.fpscr;
                        match insn.mnemonic {
                            Mnemonic::VMAX => vcvt_f16_bits_f32(
                                f32::from_bits(Self::neon_fpmax_f32_bits(n_val, m_val)),
                                &mut fpscr,
                            ) as u64,
                            Mnemonic::VMIN => vcvt_f16_bits_f32(
                                f32::from_bits(Self::neon_fpmin_f32_bits(n_val, m_val)),
                                &mut fpscr,
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
                let condition = match insn.mnemonic {
                    Mnemonic::VCEQ => lhs == rhs,
                    Mnemonic::VCGT | Mnemonic::VACGT => lhs > rhs,
                    Mnemonic::VCGE | Mnemonic::VACGE => lhs >= rhs,
                    _ => return ExecResult::Undefined,
                };
                out.push(if condition { true_mask } else { 0 });
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        ExecResult::Continue
    }

    pub(crate) fn neon_fpmax_f32_bits(a: f32, b: f32) -> u32 {
        if a.is_nan() || b.is_nan() {
            return f32::NAN.to_bits();
        }
        if a == b {
            if a.is_sign_positive() || b.is_sign_positive() {
                0.0f32.to_bits()
            } else {
                a.to_bits()
            }
        } else {
            a.max(b).to_bits()
        }
    }

    pub(crate) fn neon_fpmin_f32_bits(a: f32, b: f32) -> u32 {
        if a.is_nan() || b.is_nan() {
            return f32::NAN.to_bits();
        }
        if a == b {
            if a.is_sign_negative() || b.is_sign_negative() {
                (-0.0f32).to_bits()
            } else {
                a.to_bits()
            }
        } else {
            a.min(b).to_bits()
        }
    }

    pub(crate) fn exec_neon_fp_absdiff(&mut self, insn: &DecodedInsn) -> ExecResult {
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
                        (n_val - m_val).abs().to_bits() as u64
                    }
                    NeonSize::H16 => {
                        let n_val = vcvt_f32_f16_bits(n_elem as u16);
                        let m_val = vcvt_f32_f16_bits(m_elem as u16);
                        let mut fpscr = self.cpu.vfp.fpscr;
                        vcvt_f16_bits_f32((n_val - m_val).abs(), &mut fpscr) as u64
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        ExecResult::Continue
    }

    pub(crate) fn exec_vrint(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        let Some((d, m, size)) = self.decode_vfp_unary_regs(insn) else {
            return ExecResult::Undefined;
        };
        let Some((mode, exact)) = self.vrint_rounding(insn.mnemonic) else {
            return ExecResult::Undefined;
        };

        match size {
            16 => {
                let value = self.cpu.vfp.read_h_bits(m);
                let result = vrint_f16_bits(value, mode, exact, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_h_bits(d, result);
            }
            32 => {
                let value = self.cpu.vfp.read_s(m);
                let result = vrint_f32(value, mode, exact, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s(d, result);
            }
            64 => {
                let value = self.cpu.vfp.read_d(m);
                let result = vrint_f64(value, mode, exact, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_d(d, result);
            }
            _ => return ExecResult::Undefined,
        }

        ExecResult::Continue
    }

    pub(crate) fn exec_vcvt(&mut self, insn: &DecodedInsn) -> ExecResult {
        if Self::is_neon_fp16_convert_shape(insn.raw) {
            return self.exec_neon_fp16_convert(insn);
        }

        if Self::is_neon_fp_fixed_convert_shape(insn.raw) {
            return self.exec_neon_fp_fixed_convert(insn);
        }

        if Self::is_neon_fp_convert_shape(insn.raw) {
            return self.exec_neon_fp_convert(insn);
        }

        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        let Some((d, m)) = self.decode_vcvt_regs(insn) else {
            return ExecResult::Undefined;
        };

        match insn.mnemonic {
            Mnemonic::VCVT_F32_S32 => {
                let value = self.cpu.vfp.read_s_bits(m) as i32;
                self.cpu.vfp.write_s(d, vcvt_f32_s32(value));
            }
            Mnemonic::VCVT_F32_U32 => {
                let value = self.cpu.vfp.read_s_bits(m);
                self.cpu.vfp.write_s(d, vcvt_f32_u32(value));
            }
            Mnemonic::VCVT_F16_S32 => {
                let value = self.cpu.vfp.read_s_bits(m) as i32;
                let bits = vcvt_f16_bits_f32(vcvt_f32_s32(value), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_h_bits(d, bits);
            }
            Mnemonic::VCVT_F16_U32 => {
                let value = self.cpu.vfp.read_s_bits(m);
                let bits = vcvt_f16_bits_f32(vcvt_f32_u32(value), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_h_bits(d, bits);
            }
            Mnemonic::VCVT_S32_F32 => {
                let value = vcvt_s32_f32(self.cpu.vfp.read_s(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVT_U32_F32 => {
                let value = vcvt_u32_f32(self.cpu.vfp.read_s(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVT_S32_F16 => {
                let value = vcvt_s32_f32(
                    vcvt_f32_f16_bits(self.cpu.vfp.read_h_bits(m)),
                    &mut self.cpu.vfp.fpscr,
                );
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVT_U32_F16 => {
                let value = vcvt_u32_f32(
                    vcvt_f32_f16_bits(self.cpu.vfp.read_h_bits(m)),
                    &mut self.cpu.vfp.fpscr,
                );
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVTR_S32_F32 => {
                let value = vcvtr_s32_f32(self.cpu.vfp.read_s(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVTR_U32_F32 => {
                let value = vcvtr_u32_f32(self.cpu.vfp.read_s(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVTR_S32_F16 => {
                let value = vcvtr_s32_f32(
                    vcvt_f32_f16_bits(self.cpu.vfp.read_h_bits(m)),
                    &mut self.cpu.vfp.fpscr,
                );
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVTR_U32_F16 => {
                let value = vcvtr_u32_f32(
                    vcvt_f32_f16_bits(self.cpu.vfp.read_h_bits(m)),
                    &mut self.cpu.vfp.fpscr,
                );
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVT_F64_S32 => {
                let value = self.cpu.vfp.read_s_bits(m) as i32;
                self.cpu.vfp.write_d(d, vcvt_f64_s32(value));
            }
            Mnemonic::VCVT_F64_U32 => {
                let value = self.cpu.vfp.read_s_bits(m);
                self.cpu.vfp.write_d(d, vcvt_f64_u32(value));
            }
            Mnemonic::VCVT_S32_F64 => {
                let value = vcvt_s32_f64(self.cpu.vfp.read_d(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVT_U32_F64 => {
                let value = vcvt_u32_f64(self.cpu.vfp.read_d(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVTR_S32_F64 => {
                let value = vcvtr_s32_f64(self.cpu.vfp.read_d(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVTR_U32_F64 => {
                let value = vcvtr_u32_f64(self.cpu.vfp.read_d(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVTA_S32_F32
            | Mnemonic::VCVTM_S32_F32
            | Mnemonic::VCVTN_S32_F32
            | Mnemonic::VCVTP_S32_F32 => {
                let Some(mode) = Self::directed_vcvt_rounding(insn.mnemonic) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_s32_f32_round(self.cpu.vfp.read_s(m), mode, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVTA_S32_F16
            | Mnemonic::VCVTM_S32_F16
            | Mnemonic::VCVTN_S32_F16
            | Mnemonic::VCVTP_S32_F16 => {
                let Some(mode) = Self::directed_vcvt_rounding(insn.mnemonic) else {
                    return ExecResult::Undefined;
                };
                let value = vcvt_s32_f32_round(
                    vcvt_f32_f16_bits(self.cpu.vfp.read_h_bits(m)),
                    mode,
                    &mut self.cpu.vfp.fpscr,
                );
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVTA_U32_F32
            | Mnemonic::VCVTM_U32_F32
            | Mnemonic::VCVTN_U32_F32
            | Mnemonic::VCVTP_U32_F32 => {
                let Some(mode) = Self::directed_vcvt_rounding(insn.mnemonic) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_u32_f32_round(self.cpu.vfp.read_s(m), mode, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVTA_U32_F16
            | Mnemonic::VCVTM_U32_F16
            | Mnemonic::VCVTN_U32_F16
            | Mnemonic::VCVTP_U32_F16 => {
                let Some(mode) = Self::directed_vcvt_rounding(insn.mnemonic) else {
                    return ExecResult::Undefined;
                };
                let value = vcvt_u32_f32_round(
                    vcvt_f32_f16_bits(self.cpu.vfp.read_h_bits(m)),
                    mode,
                    &mut self.cpu.vfp.fpscr,
                );
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVTA_S32_F64
            | Mnemonic::VCVTM_S32_F64
            | Mnemonic::VCVTN_S32_F64
            | Mnemonic::VCVTP_S32_F64 => {
                let Some(mode) = Self::directed_vcvt_rounding(insn.mnemonic) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_s32_f64_round(self.cpu.vfp.read_d(m), mode, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVTA_U32_F64
            | Mnemonic::VCVTM_U32_F64
            | Mnemonic::VCVTN_U32_F64
            | Mnemonic::VCVTP_U32_F64 => {
                let Some(mode) = Self::directed_vcvt_rounding(insn.mnemonic) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_u32_f64_round(self.cpu.vfp.read_d(m), mode, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVT_F64_F32 => {
                self.cpu
                    .vfp
                    .write_d(d, vcvt_f64_f32(self.cpu.vfp.read_s(m)));
            }
            Mnemonic::VCVT_F32_F64 => {
                let value = vcvt_f32_f64(self.cpu.vfp.read_d(m), &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s(d, value);
            }
            Mnemonic::VCVTB_F32_F16 | Mnemonic::VCVTT_F32_F16 => {
                let shift = if insn.mnemonic == Mnemonic::VCVTT_F32_F16 {
                    16
                } else {
                    0
                };
                let bits = (self.cpu.vfp.read_s_bits(m) >> shift) as u16;
                self.cpu.vfp.write_s(d, vcvt_f32_f16_bits(bits));
            }
            Mnemonic::VCVTB_F16_F32 | Mnemonic::VCVTT_F16_F32 => {
                let shift = if insn.mnemonic == Mnemonic::VCVTT_F16_F32 {
                    16
                } else {
                    0
                };
                let value = vcvt_f16_bits_f32(self.cpu.vfp.read_s(m), &mut self.cpu.vfp.fpscr);
                let old = self.cpu.vfp.read_s_bits(d);
                let mask = 0xFFFFu32 << shift;
                self.cpu
                    .vfp
                    .write_s_bits(d, (old & !mask) | ((value as u32) << shift));
            }
            Mnemonic::VCVT_F32_S32_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value = self.cpu.vfp.read_s_bits(d) as i32;
                self.cpu.vfp.write_s(d, vcvt_f32_s32_fixed(value, fbits));
            }
            Mnemonic::VCVT_F32_U32_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value = self.cpu.vfp.read_s_bits(d);
                self.cpu.vfp.write_s(d, vcvt_f32_u32_fixed(value, fbits));
            }
            Mnemonic::VCVT_S32_F32_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_s32_f32_fixed(self.cpu.vfp.read_s(d), fbits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value as u32);
            }
            Mnemonic::VCVT_U32_F32_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_u32_f32_fixed(self.cpu.vfp.read_s(d), fbits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, value);
            }
            Mnemonic::VCVT_F64_S32_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value = self.cpu.vfp.read_d_bits(d) as u32 as i32;
                self.cpu.vfp.write_d(d, vcvt_f64_s32_fixed(value, fbits));
            }
            Mnemonic::VCVT_F64_U32_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value = self.cpu.vfp.read_d_bits(d) as u32;
                self.cpu.vfp.write_d(d, vcvt_f64_u32_fixed(value, fbits));
            }
            Mnemonic::VCVT_S32_F64_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_s32_f64_fixed(self.cpu.vfp.read_d(d), fbits, &mut self.cpu.vfp.fpscr);
                let old = self.cpu.vfp.read_d_bits(d) & 0xFFFF_FFFF_0000_0000;
                self.cpu.vfp.write_d_bits(d, old | (value as u32 as u64));
            }
            Mnemonic::VCVT_U32_F64_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_u32_f64_fixed(self.cpu.vfp.read_d(d), fbits, &mut self.cpu.vfp.fpscr);
                let old = self.cpu.vfp.read_d_bits(d) & 0xFFFF_FFFF_0000_0000;
                self.cpu.vfp.write_d_bits(d, old | value as u64);
            }
            _ => return ExecResult::Undefined,
        }

        ExecResult::Continue
    }

    pub(crate) fn is_neon_fp_convert_shape(raw: u32) -> bool {
        (raw >> 25) == 0b1111001
            && ((raw >> 24) & 1) == 1
            && ((raw >> 23) & 1) == 1
            && ((raw >> 21) & 0x7) == 0b101
            && ((raw >> 20) & 1) == 1
            && ((raw >> 16) & 0xF) == 0b1011
            && ((raw >> 8) & 0xE) == 0b0110
            && ((raw >> 5) & 1) == 0
            && ((raw >> 4) & 1) == 0
    }

    pub(crate) fn is_neon_fp16_convert_shape(raw: u32) -> bool {
        (raw >> 25) == 0b1111001
            && ((raw >> 24) & 1) == 1
            && ((raw >> 23) & 1) == 1
            && ((raw >> 20) & 0x7) == 0b011
            && ((raw >> 16) & 0xF) == 0b0110
            && matches!((raw >> 8) & 0xF, 0b0110 | 0b0111)
            && ((raw >> 7) & 1) == 0
            && ((raw >> 6) & 1) == 0
            && ((raw >> 5) & 1) == 0
            && ((raw >> 4) & 1) == 0
    }

    pub(crate) fn is_neon_fp_fixed_convert_shape(raw: u32) -> bool {
        (raw >> 25) == 0b1111001
            && ((raw >> 23) & 1) == 1
            && ((raw >> 8) & 0xE) == 0b1110
            && ((raw >> 7) & 1) == 0
            && ((raw >> 4) & 1) == 1
            && ((raw >> 16) & 0x3F) >= 32
    }

    pub(crate) fn exec_neon_fp_convert(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !Self::is_neon_fp_convert_shape(insn.raw) {
            return ExecResult::Undefined;
        }

        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let q = ((insn.raw >> 6) & 1) != 0;
        let regs = if q { 2 } else { 1 };
        let d = (d_bit << 4) | vd;
        let m = (m_bit << 4) | vm;
        if q && ((d | m) & 1) != 0 {
            return ExecResult::Undefined;
        }
        if d + regs > 32 || m + regs > 32 {
            return ExecResult::Undefined;
        }

        for reg in 0..regs {
            let elements = self.neon_read_vector_elements_u64(m + reg, 1, 4);
            let mut out = Vec::with_capacity(elements.len());
            for elem in elements {
                let result = match insn.mnemonic {
                    Mnemonic::VCVT_F32_S32 => u64::from(vcvt_f32_s32(elem as u32 as i32).to_bits()),
                    Mnemonic::VCVT_F32_U32 => u64::from(vcvt_f32_u32(elem as u32).to_bits()),
                    Mnemonic::VCVT_S32_F32 => {
                        let value =
                            vcvt_s32_f32(f32::from_bits(elem as u32), &mut self.cpu.vfp.fpscr);
                        u64::from(value as u32)
                    }
                    Mnemonic::VCVT_U32_F32 => {
                        let value =
                            vcvt_u32_f32(f32::from_bits(elem as u32), &mut self.cpu.vfp.fpscr);
                        u64::from(value)
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, 4, &out);
        }

        ExecResult::Continue
    }

    pub(crate) fn exec_neon_fp16_convert(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !Self::is_neon_fp16_convert_shape(insn.raw) {
            return ExecResult::Undefined;
        }

        let d = ((((insn.raw >> 22) & 1) << 4) | ((insn.raw >> 12) & 0xF)) as u8;
        let m = (insn.raw & 0xF) as u8;
        match insn.mnemonic {
            Mnemonic::VCVT_F16_F32 => {
                if (m & 1) != 0 || m + 1 >= 32 {
                    return ExecResult::Undefined;
                }
                let values = self.neon_read_vector_elements_u64(m, 2, 4);
                let mut out = Vec::with_capacity(values.len());
                for elem in values {
                    let value =
                        vcvt_f16_bits_f32(f32::from_bits(elem as u32), &mut self.cpu.vfp.fpscr);
                    out.push(u64::from(value));
                }
                self.neon_write_vector_elements_u64(d, 1, 2, &out);
            }
            Mnemonic::VCVT_F32_F16 => {
                if (d & 1) != 0 || d + 1 >= 32 {
                    return ExecResult::Undefined;
                }
                let values = self.neon_read_vector_elements_u64(m, 1, 2);
                let mut out = Vec::with_capacity(values.len());
                for elem in values {
                    out.push(u64::from(vcvt_f32_f16_bits(elem as u16).to_bits()));
                }
                self.neon_write_vector_elements_u64(d, 2, 4, &out);
            }
            _ => return ExecResult::Undefined,
        }

        ExecResult::Continue
    }

    pub(crate) fn exec_neon_fp_fixed_convert(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !Self::is_neon_fp_fixed_convert_shape(insn.raw) {
            return ExecResult::Undefined;
        }

        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let q = ((insn.raw >> 6) & 1) != 0;
        let regs = if q { 2 } else { 1 };
        let d = (d_bit << 4) | vd;
        let m = (m_bit << 4) | vm;
        if q && ((d | m) & 1) != 0 {
            return ExecResult::Undefined;
        }
        if d + regs > 32 || m + regs > 32 {
            return ExecResult::Undefined;
        }

        let fbits = 64 - ((insn.raw >> 16) & 0x3F);
        for reg in 0..regs {
            let elements = self.neon_read_vector_elements_u64(m + reg, 1, 4);
            let mut out = Vec::with_capacity(elements.len());
            for elem in elements {
                let result = match insn.mnemonic {
                    Mnemonic::VCVT_F32_S32_FIXED => {
                        u64::from(vcvt_f32_s32_fixed(elem as u32 as i32, fbits).to_bits())
                    }
                    Mnemonic::VCVT_F32_U32_FIXED => {
                        u64::from(vcvt_f32_u32_fixed(elem as u32, fbits).to_bits())
                    }
                    Mnemonic::VCVT_S32_F32_FIXED => {
                        let value = vcvt_s32_f32_fixed(
                            f32::from_bits(elem as u32),
                            fbits,
                            &mut self.cpu.vfp.fpscr,
                        );
                        u64::from(value as u32)
                    }
                    Mnemonic::VCVT_U32_F32_FIXED => {
                        let value = vcvt_u32_f32_fixed(
                            f32::from_bits(elem as u32),
                            fbits,
                            &mut self.cpu.vfp.fpscr,
                        );
                        u64::from(value)
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, 4, &out);
        }

        ExecResult::Continue
    }

    pub(crate) fn neon_float_to_int_lane(
        value: f32,
        bits: u32,
        unsigned: bool,
        mode: RoundingMode,
        fpscr: &mut Fpscr,
    ) -> u32 {
        let rounded = match mode {
            RoundingMode::RoundNearest => value.round_ties_even(),
            RoundingMode::RoundPlusInf => value.ceil(),
            RoundingMode::RoundMinusInf => value.floor(),
            RoundingMode::RoundZero => value.trunc(),
            RoundingMode::RoundTiesAway => value.round(),
        };

        if unsigned {
            let max = (1u32 << bits) - 1;
            if rounded.is_nan() || rounded < 0.0 {
                fpscr.set_ioc(true);
                0
            } else if rounded >= max as f32 {
                fpscr.set_ioc(true);
                max
            } else {
                rounded as u32
            }
        } else {
            let min = -(1i32 << (bits - 1));
            let max = (1i32 << (bits - 1)) - 1;
            if rounded.is_nan() {
                fpscr.set_ioc(true);
                0
            } else if rounded >= max as f32 {
                fpscr.set_ioc(true);
                max as u32
            } else if rounded <= min as f32 {
                fpscr.set_ioc(true);
                (min as u32) & ((1u32 << bits) - 1)
            } else {
                (rounded as i32 as u32) & ((1u32 << bits) - 1)
            }
        }
    }

    pub(crate) fn directed_vcvt_rounding(mnemonic: Mnemonic) -> Option<RoundingMode> {
        match mnemonic {
            Mnemonic::VCVTA_S32_F32
            | Mnemonic::VCVTA_S32_F16
            | Mnemonic::VCVTA_U32_F32
            | Mnemonic::VCVTA_U32_F16
            | Mnemonic::VCVTA_S32_F64
            | Mnemonic::VCVTA_U32_F64 => Some(RoundingMode::RoundTiesAway),
            Mnemonic::VCVTN_S32_F32
            | Mnemonic::VCVTN_S32_F16
            | Mnemonic::VCVTN_U32_F32
            | Mnemonic::VCVTN_U32_F16
            | Mnemonic::VCVTN_S32_F64
            | Mnemonic::VCVTN_U32_F64 => Some(RoundingMode::RoundNearest),
            Mnemonic::VCVTP_S32_F32
            | Mnemonic::VCVTP_S32_F16
            | Mnemonic::VCVTP_U32_F32
            | Mnemonic::VCVTP_U32_F16
            | Mnemonic::VCVTP_S32_F64
            | Mnemonic::VCVTP_U32_F64 => Some(RoundingMode::RoundPlusInf),
            Mnemonic::VCVTM_S32_F32
            | Mnemonic::VCVTM_S32_F16
            | Mnemonic::VCVTM_U32_F32
            | Mnemonic::VCVTM_U32_F16
            | Mnemonic::VCVTM_S32_F64
            | Mnemonic::VCVTM_U32_F64 => Some(RoundingMode::RoundMinusInf),
            _ => None,
        }
    }

    pub(crate) fn decode_vcvt_regs(&self, insn: &DecodedInsn) -> Option<(u8, u8)> {
        let d_bit = ((insn.raw >> 22) & 1) as u8;
        let vd = ((insn.raw >> 12) & 0xF) as u8;
        let m_bit = ((insn.raw >> 5) & 1) as u8;
        let vm = (insn.raw & 0xF) as u8;
        let d_s = (vd << 1) | d_bit;
        let d_d = (d_bit << 4) | vd;
        let m_s = (vm << 1) | m_bit;
        let m_d = (m_bit << 4) | vm;

        match insn.mnemonic {
            Mnemonic::VCVT_F32_S32
            | Mnemonic::VCVT_F32_U32
            | Mnemonic::VCVT_F16_S32
            | Mnemonic::VCVT_F16_U32
            | Mnemonic::VCVT_S32_F32
            | Mnemonic::VCVT_U32_F32
            | Mnemonic::VCVT_S32_F16
            | Mnemonic::VCVT_U32_F16
            | Mnemonic::VCVTR_S32_F32
            | Mnemonic::VCVTR_U32_F32
            | Mnemonic::VCVTR_S32_F16
            | Mnemonic::VCVTR_U32_F16
            | Mnemonic::VCVTA_S32_F32
            | Mnemonic::VCVTA_U32_F32
            | Mnemonic::VCVTA_S32_F16
            | Mnemonic::VCVTA_U32_F16
            | Mnemonic::VCVTM_S32_F32
            | Mnemonic::VCVTM_U32_F32
            | Mnemonic::VCVTM_S32_F16
            | Mnemonic::VCVTM_U32_F16
            | Mnemonic::VCVTN_S32_F32
            | Mnemonic::VCVTN_U32_F32
            | Mnemonic::VCVTN_S32_F16
            | Mnemonic::VCVTN_U32_F16
            | Mnemonic::VCVTP_S32_F32
            | Mnemonic::VCVTP_U32_F32
            | Mnemonic::VCVTP_S32_F16
            | Mnemonic::VCVTP_U32_F16
            | Mnemonic::VCVTB_F32_F16
            | Mnemonic::VCVTT_F32_F16
            | Mnemonic::VCVTB_F16_F32
            | Mnemonic::VCVTT_F16_F32 => Some((d_s, m_s)),
            Mnemonic::VCVT_F32_S32_FIXED
            | Mnemonic::VCVT_F32_U32_FIXED
            | Mnemonic::VCVT_S32_F32_FIXED
            | Mnemonic::VCVT_U32_F32_FIXED => Some((d_s, d_s)),
            Mnemonic::VCVT_F64_S32 | Mnemonic::VCVT_F64_U32 | Mnemonic::VCVT_F64_F32 => {
                Some((d_d, m_s))
            }
            Mnemonic::VCVT_F64_S32_FIXED
            | Mnemonic::VCVT_F64_U32_FIXED
            | Mnemonic::VCVT_S32_F64_FIXED
            | Mnemonic::VCVT_U32_F64_FIXED => Some((d_d, d_d)),
            Mnemonic::VCVT_S32_F64
            | Mnemonic::VCVT_U32_F64
            | Mnemonic::VCVT_F32_F64
            | Mnemonic::VCVTR_S32_F64
            | Mnemonic::VCVTR_U32_F64
            | Mnemonic::VCVTA_S32_F64
            | Mnemonic::VCVTA_U32_F64
            | Mnemonic::VCVTM_S32_F64
            | Mnemonic::VCVTM_U32_F64
            | Mnemonic::VCVTN_S32_F64
            | Mnemonic::VCVTN_U32_F64
            | Mnemonic::VCVTP_S32_F64
            | Mnemonic::VCVTP_U32_F64 => Some((d_s, m_d)),
            _ => None,
        }
    }

    pub(crate) fn decode_vcvt_fixed_fbits(insn: &DecodedInsn) -> Option<u32> {
        if ((insn.raw >> 7) & 1) == 0 {
            return None;
        }
        let imm5 = ((insn.raw & 0xF) << 1) | ((insn.raw >> 5) & 1);
        Some(32 - imm5)
    }
}
