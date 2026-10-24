//! fp.rs
use crate::isa::arm::aarch32::vfp::{
    vcvt_f16_bits_s32, vcvt_f16_bits_u32, vcvt_f32_f16_bits_with_fpscr,
};

use crate::isa::arm::ExecutionState;
use crate::isa::arm::aarch32::cpu::{
    ArmMemory, Armv7Cpu, MemoryError, ProcessorMode, Psr, add_with_carry, compute_n_flag,
    compute_z_flag, condition_passed, expand_imm_c, shift_c, sign_extend,
};
use crate::isa::arm::aarch32::instructions::neon::*;
use crate::isa::arm::aarch32::instructions::*;
use crate::isa::arm::aarch32::vfp::{
    Fpscr, NeonSize, RoundingMode, vadd_f16_bits, vadd_f32, vcvt_f16_bits_f32, vcvt_f32_f16_bits,
    vcvt_f32_f64, vcvt_f32_s32, vcvt_f32_s32_fixed_with_fpscr, vcvt_f32_s32_with_fpscr,
    vcvt_f32_u32, vcvt_f32_u32_fixed_with_fpscr, vcvt_f32_u32_with_fpscr,
    vcvt_f64_s32_fixed_with_fpscr, vcvt_f64_s32_with_fpscr, vcvt_f64_u32_fixed_with_fpscr,
    vcvt_f64_u32_with_fpscr, vcvt_s32_f32, vcvt_s32_f32_fixed, vcvt_s32_f32_round, vcvt_s32_f64,
    vcvt_s32_f64_fixed, vcvt_s32_f64_round, vcvt_u32_f32, vcvt_u32_f32_fixed, vcvt_u32_f32_round,
    vcvt_u32_f64, vcvt_u32_f64_fixed, vcvt_u32_f64_round, vcvtr_s32_f32, vcvtr_s32_f64,
    vcvtr_u32_f32, vcvtr_u32_f64, vfma_f32, vfms_f32, vmla_f16_bits, vmla_f32, vmls_f16_bits,
    vmls_f32, vmul_f16_bits, vmul_f32, vrint_f16_bits, vrint_f32, vrint_f64, vsub_f16_bits,
    vsub_f32,
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
        self.with_standard_fp_controls(|executor| executor.exec_neon_fp_add_sub_inner(insn))
    }

    fn exec_neon_fp_add_sub_inner(&mut self, insn: &DecodedInsn) -> ExecResult {
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
        self.with_standard_fp_controls(|executor| executor.exec_neon_fp_pairwise_inner(insn))
    }

    fn exec_neon_fp_pairwise_inner(&mut self, insn: &DecodedInsn) -> ExecResult {
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
                            Mnemonic::VPMAX => Self::neon_fpmax_f32_bits(lhs, rhs, fpscr),
                            Mnemonic::VPMIN => Self::neon_fpmin_f32_bits(lhs, rhs, fpscr),
                            _ => return ExecResult::Undefined,
                        })
                    }
                    NeonSize::H16 => {
                        let lhs = pair[0] as u16;
                        let rhs = pair[1] as u16;
                        u64::from(match insn.mnemonic {
                            Mnemonic::VPADD => vadd_f16_bits(lhs, rhs, fpscr),
                            Mnemonic::VPMAX | Mnemonic::VPMIN => Self::neon_fp_minmax_f16_bits(
                                lhs,
                                rhs,
                                insn.mnemonic == Mnemonic::VPMAX,
                                fpscr,
                            ),
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
        let Some((mut mode, exact)) = self.vrint_rounding(insn.mnemonic) else {
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

        if matches!(insn.mnemonic, Mnemonic::VRINTX_F32 | Mnemonic::VRINTX_F16) {
            mode = RoundingMode::RoundNearest;
        }
        let mut fpscr = self.standard_simd_fp_controls();
        let ebytes = (size.bits() / 8) as u8;
        for reg in 0..regs {
            let elements = self.neon_read_vector_elements_u64(m + reg, 1, ebytes);
            let mut out = Vec::with_capacity(elements.len());
            for elem in elements {
                let result = match size {
                    NeonSize::H16 => {
                        u64::from(vrint_f16_bits(elem as u16, mode, exact, &mut fpscr))
                    }
                    NeonSize::S32 => u64::from(
                        vrint_f32(f32::from_bits(elem as u32), mode, exact, &mut fpscr).to_bits(),
                    ),
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        self.cpu.vfp.fpscr = Fpscr::from_bits(self.cpu.vfp.fpscr.bits() | (fpscr.bits() & 0x9f));
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
        self.with_standard_fp_controls(|executor| executor.exec_neon_fp_multiply_inner(insn))
    }

    fn exec_neon_fp_multiply_inner(&mut self, insn: &DecodedInsn) -> ExecResult {
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
                self.cpu.vfp.write_s_bits(d, u32::from(result));
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
                let result = vcvt_f32_s32_with_fpscr(value, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s(d, result);
            }
            Mnemonic::VCVT_F32_U32 => {
                let value = self.cpu.vfp.read_s_bits(m);
                let result = vcvt_f32_u32_with_fpscr(value, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s(d, result);
            }
            Mnemonic::VCVT_F16_S32 => {
                let value = self.cpu.vfp.read_s_bits(m) as i32;
                let bits = vcvt_f16_bits_s32(value, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, u32::from(bits));
            }
            Mnemonic::VCVT_F16_U32 => {
                let value = self.cpu.vfp.read_s_bits(m);
                let bits = vcvt_f16_bits_u32(value, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s_bits(d, u32::from(bits));
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
                let result = vcvt_f64_s32_with_fpscr(value, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_d(d, result);
            }
            Mnemonic::VCVT_F64_U32 => {
                let value = self.cpu.vfp.read_s_bits(m);
                let result = vcvt_f64_u32_with_fpscr(value, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_d(d, result);
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
                let value = crate::isa::arm::aarch32::vfp::vcvt_f64_f32_with_fpscr(
                    self.cpu.vfp.read_s(m),
                    &mut self.cpu.vfp.fpscr,
                );
                self.cpu.vfp.write_d(d, value);
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
                let value = vcvt_f32_f16_bits_with_fpscr(bits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s(d, value);
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
                let result = vcvt_f32_s32_fixed_with_fpscr(value, fbits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s(d, result);
            }
            Mnemonic::VCVT_F32_U32_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value = self.cpu.vfp.read_s_bits(d);
                let result = vcvt_f32_u32_fixed_with_fpscr(value, fbits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_s(d, result);
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
                let result = vcvt_f64_s32_fixed_with_fpscr(value, fbits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_d(d, result);
            }
            Mnemonic::VCVT_F64_U32_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value = self.cpu.vfp.read_d_bits(d) as u32;
                let result = vcvt_f64_u32_fixed_with_fpscr(value, fbits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_d(d, result);
            }
            Mnemonic::VCVT_S32_F64_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_s32_f64_fixed(self.cpu.vfp.read_d(d), fbits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_d_bits(d, value as i64 as u64);
            }
            Mnemonic::VCVT_U32_F64_FIXED => {
                let Some(fbits) = Self::decode_vcvt_fixed_fbits(insn) else {
                    return ExecResult::Undefined;
                };
                let value =
                    vcvt_u32_f64_fixed(self.cpu.vfp.read_d(d), fbits, &mut self.cpu.vfp.fpscr);
                self.cpu.vfp.write_d_bits(d, u64::from(value));
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
            && ((raw >> 20) & 0x3) == 0b11
            && ((raw >> 16) & 0xF) == 0b0110
            && matches!((raw >> 8) & 0xF, 0b0110 | 0b0111)
            && ((raw >> 7) & 1) == 0
            && ((raw >> 6) & 1) == 0
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

    pub(super) fn standard_simd_fp_controls(&self) -> Fpscr {
        // StandardFPSCRValue fixes RM/DN/FZ and preserves AHP/FZ16 for
        // operation-specific half precision and data-processing profiles.
        Fpscr::from_bits((self.cpu.vfp.fpscr.bits() & ((1 << 26) | (1 << 19))) | 0x0300_0000)
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

        let mut fpscr = self.standard_simd_fp_controls();
        for reg in 0..regs {
            let elements = self.neon_read_vector_elements_u64(m + reg, 1, 4);
            let mut out = Vec::with_capacity(elements.len());
            for elem in elements {
                let result = match insn.mnemonic {
                    Mnemonic::VCVT_F32_S32 => {
                        u64::from(vcvt_f32_s32_with_fpscr(elem as u32 as i32, &mut fpscr).to_bits())
                    }
                    Mnemonic::VCVT_F32_U32 => {
                        u64::from(vcvt_f32_u32_with_fpscr(elem as u32, &mut fpscr).to_bits())
                    }
                    Mnemonic::VCVT_S32_F32 => {
                        let value = vcvt_s32_f32(f32::from_bits(elem as u32), &mut fpscr);
                        u64::from(value as u32)
                    }
                    Mnemonic::VCVT_U32_F32 => {
                        let value = vcvt_u32_f32(f32::from_bits(elem as u32), &mut fpscr);
                        u64::from(value)
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, 4, &out);
        }

        self.cpu.vfp.fpscr = Fpscr::from_bits(self.cpu.vfp.fpscr.bits() | (fpscr.bits() & 0x9f));
        ExecResult::Continue
    }

    pub(crate) fn exec_neon_fp16_convert(&mut self, insn: &DecodedInsn) -> ExecResult {
        self.with_standard_fp_controls(|executor| executor.exec_neon_fp16_convert_inner(insn))
    }
    fn exec_neon_fp16_convert_inner(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !Self::is_neon_fp16_convert_shape(insn.raw) {
            return ExecResult::Undefined;
        }

        let d = ((((insn.raw >> 22) & 1) << 4) | ((insn.raw >> 12) & 0xF)) as u8;
        let m = ((((insn.raw >> 5) & 1) << 4) | (insn.raw & 0xF)) as u8;
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
                    out.push(u64::from(
                        vcvt_f32_f16_bits_with_fpscr(elem as u16, &mut self.cpu.vfp.fpscr)
                            .to_bits(),
                    ));
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
        let mut fpscr = self.standard_simd_fp_controls();
        for reg in 0..regs {
            let elements = self.neon_read_vector_elements_u64(m + reg, 1, 4);
            let mut out = Vec::with_capacity(elements.len());
            for elem in elements {
                let result = match insn.mnemonic {
                    Mnemonic::VCVT_F32_S32_FIXED => u64::from(
                        vcvt_f32_s32_fixed_with_fpscr(elem as u32 as i32, fbits, &mut fpscr)
                            .to_bits(),
                    ),
                    Mnemonic::VCVT_F32_U32_FIXED => u64::from(
                        vcvt_f32_u32_fixed_with_fpscr(elem as u32, fbits, &mut fpscr).to_bits(),
                    ),
                    Mnemonic::VCVT_S32_F32_FIXED => {
                        let value =
                            vcvt_s32_f32_fixed(f32::from_bits(elem as u32), fbits, &mut fpscr);
                        u64::from(value as u32)
                    }
                    Mnemonic::VCVT_U32_F32_FIXED => {
                        let value =
                            vcvt_u32_f32_fixed(f32::from_bits(elem as u32), fbits, &mut fpscr);
                        u64::from(value)
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, 4, &out);
        }

        self.cpu.vfp.fpscr = Fpscr::from_bits(self.cpu.vfp.fpscr.bits() | (fpscr.bits() & 0x9f));
        ExecResult::Continue
    }

    pub(crate) fn exec_neon_directed_convert(&mut self, insn: &DecodedInsn) -> ExecResult {
        if !self.cpu.vfp.is_enabled() {
            return ExecResult::Exception(ExceptionType::UndefinedInstruction);
        }
        if !Self::is_neon_directed_convert_shape(insn.raw) {
            return ExecResult::Undefined;
        }

        let size = match (insn.raw >> 18) & 0x3 {
            0b01 => NeonSize::H16,
            0b10 => NeonSize::S32,
            _ => return ExecResult::Undefined,
        };
        let unsigned = ((insn.raw >> 7) & 1) != 0;
        let mode = match (insn.raw >> 8) & 0x3 {
            0b00 => RoundingMode::RoundTiesAway,
            0b01 => RoundingMode::RoundNearest,
            0b10 => RoundingMode::RoundPlusInf,
            0b11 => RoundingMode::RoundMinusInf,
            _ => unreachable!(),
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

        let mut fpscr = if size == NeonSize::S32 {
            self.standard_simd_fp_controls()
        } else {
            self.cpu.vfp.fpscr
        };
        let ebytes = (size.bits() / 8) as u8;
        for reg in 0..regs {
            let elements = self.neon_read_vector_elements_u64(m + reg, 1, ebytes);
            let mut out = Vec::with_capacity(elements.len());
            for elem in elements {
                let result = match size {
                    NeonSize::H16 => {
                        let value = vcvt_f32_f16_bits(elem as u16);
                        u64::from(Self::neon_float_to_int_lane(
                            value, 16, unsigned, mode, &mut fpscr,
                        ))
                    }
                    NeonSize::S32 if unsigned => {
                        let value =
                            vcvt_u32_f32_round(f32::from_bits(elem as u32), mode, &mut fpscr);
                        u64::from(value)
                    }
                    NeonSize::S32 => {
                        let value =
                            vcvt_s32_f32_round(f32::from_bits(elem as u32), mode, &mut fpscr);
                        u64::from(value as u32)
                    }
                    _ => return ExecResult::Undefined,
                };
                out.push(result);
            }
            self.neon_write_vector_elements_u64(d + reg, 1, ebytes, &out);
        }

        self.cpu.vfp.fpscr = Fpscr::from_bits(self.cpu.vfp.fpscr.bits() | (fpscr.bits() & 0x9f));
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
