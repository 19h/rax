//! Vector-state preservation across native-to-Rust helper boundaries.

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::ops::{X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{DispSize, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    X86_GUEST_K_OFFSET, X86_GUEST_MXCSR_OFFSET, X86_GUEST_VECTOR_SCRATCH_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET,
};

impl X86_64Lowerer {
    /// Select the AVX-only helper boundary used by YMM16-safe replay regions.
    pub fn set_avx_ymm16_vector_state(&mut self, on: bool) {
        self.avx_ymm16_vector_state = on;
    }

    fn emit_unaligned_vector_load(&mut self, register: PhysReg, width: VecWidth, offset: i32) {
        let mut emitter = X86Emitter::new(&mut self.code);
        match width {
            VecWidth::V128 | VecWidth::V256 => {
                emitter.emit_vex_prefix(
                    X86VecMap::Map0F,
                    X86SsePrefix::Rep,
                    width,
                    false,
                    register.vec_ext(),
                    0,
                    PhysReg::Rax.vec_ext(),
                    0,
                );
            }
            VecWidth::V512 => {
                emitter.emit_evex_prefix(
                    X86VecMap::Map0F,
                    X86SsePrefix::Rep,
                    width,
                    true,
                    register.vec_ext(),
                    0,
                    PhysReg::Rax.vec_ext(),
                    register.vec_ext2(),
                    0,
                    PhysReg::Rax.vec_ext2(),
                    0,
                );
            }
            _ => unreachable!("JIT vector transfer width"),
        }
        emitter.code.emit_u8(0x6F); // VMOVDQU xmm/ymm or VMOVDQU64 zmm
        emitter.emit_modrm_mem_disp(register, PhysReg::Rax, offset, DispSize::Disp32);
    }

    /// Import a helper-produced nonarchitectural value into a borrowed vector
    /// register.
    pub(crate) fn emit_jit_vector_scratch_load(&mut self, register: PhysReg, width: VecWidth) {
        self.emit_unaligned_vector_load(register, width, X86_GUEST_VECTOR_SCRATCH_OFFSET);
    }

    /// Restore the complete architectural vector register borrowed as a
    /// helper-result carrier. The AVX-only bridge owns YMM0-YMM15; the general
    /// bridge owns complete ZMM state.
    pub(crate) fn emit_jit_vector_scratch_restore(&mut self, index: u8) {
        if self.avx_ymm16_vector_state {
            self.emit_unaligned_vector_load(
                PhysReg::Ymm(index),
                VecWidth::V256,
                X86_GUEST_ZMM_OFFSET + i32::from(index) * 64,
            );
        } else {
            self.emit_unaligned_vector_load(
                PhysReg::Zmm(index),
                VecWidth::V512,
                X86_GUEST_ZMM_OFFSET + i32::from(index) * 64,
            );
        }
    }

    /// Spill or reload every host-resident architectural vector register
    /// through `GuestRegs`. `base` is RAX before a helper call and RCX after it.
    ///
    /// The general path preserves ZMM0-ZMM31 and K0-K7. The AVX-only path
    /// preserves YMM0-YMM15; upper ZMM halves and opmasks never leave the
    /// state-backed image and therefore cannot be clobbered by the host ABI.
    pub(crate) fn emit_helper_vector_state(&mut self, base: PhysReg, store: bool) {
        if self.avx_ymm16_vector_state {
            for index in 0..16u8 {
                let reg = PhysReg::Ymm(index);
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_vex_prefix(
                    X86VecMap::Map0F,
                    X86SsePrefix::Rep,
                    VecWidth::V256,
                    false,
                    reg.vec_ext(),
                    0,
                    base.vec_ext(),
                    0,
                );
                emitter.code.emit_u8(if store { 0x7F } else { 0x6F });
                emitter.emit_modrm_mem_disp(
                    reg,
                    base,
                    X86_GUEST_ZMM_OFFSET + i32::from(index) * 64,
                    DispSize::Disp32,
                );
            }
        } else {
            for index in 0..32u8 {
                let reg = PhysReg::Zmm(index);
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_prefix(
                    X86VecMap::Map0F,
                    X86SsePrefix::Rep,
                    VecWidth::V512,
                    true,
                    reg.vec_ext(),
                    0,
                    base.vec_ext(),
                    reg.vec_ext2(),
                    0,
                    base.vec_ext2(),
                    0,
                );
                emitter.code.emit_u8(if store { 0x7F } else { 0x6F });
                emitter.emit_modrm_mem_disp(
                    reg,
                    base,
                    X86_GUEST_ZMM_OFFSET / 64 + i32::from(index),
                    DispSize::Disp8,
                );
            }

            for index in 0..8u8 {
                if self.narrow_vector_opmask_helpers {
                    // KMOVW m16,k / k,m16: VEX.L0.66.0F.W0 90/91. A 16-bit
                    // memory store intentionally leaves K[63:16] untouched.
                    self.code.emit_u8(0xC5);
                    self.code.emit_u8(0xF8);
                } else {
                    // KMOVQ m64,k / k,m64: VEX.W1.0F 90/91.
                    self.code.emit_u8(0xC4);
                    self.code.emit_u8(0xE1);
                    self.code.emit_u8(0xF8);
                }
                self.code.emit_u8(if store { 0x91 } else { 0x90 });
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_modrm_mem_disp(
                    PhysReg::Xmm(index),
                    base,
                    X86_GUEST_K_OFFSET + i32::from(index) * 8,
                    DispSize::Disp32,
                );
            }
        }

        if store {
            // Capture guest MXCSR before entering a Rust helper.
            self.code.emit_u8(0x0F);
            self.code.emit_u8(0xAE);
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_modrm_mem_disp(
                PhysReg::Rbx, // STMXCSR /3
                base,
                X86_GUEST_MXCSR_OFFSET,
                DispSize::Disp32,
            );
            // Rust executes under the host thread's original MXCSR.
            self.code.emit_u8(0x0F);
            self.code.emit_u8(0xAE);
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_modrm_mem_disp(
                PhysReg::Rdx, // LDMXCSR /2
                base,
                X86_HOST_MXCSR_OFFSET,
                DispSize::Disp32,
            );
        } else {
            // Resume native guest execution under the current guest MXCSR.
            self.code.emit_u8(0x0F);
            self.code.emit_u8(0xAE);
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_modrm_mem_disp(
                PhysReg::Rdx, // LDMXCSR /2
                base,
                X86_GUEST_MXCSR_OFFSET,
                DispSize::Disp32,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avx_ymm16_helper_boundary_uses_only_vex_moves_and_mxcsr_state() {
        let mut store = X86_64Lowerer::new();
        store.set_avx_ymm16_vector_state(true);
        store.emit_helper_vector_state(PhysReg::Rax, true);
        let store = store.code.as_slice();
        assert_eq!(store.len(), 16 * 8 + 14);
        assert_eq!(&store[..8], &[0xC5, 0xFE, 0x7F, 0x80, 0x40, 0x01, 0, 0]);
        assert_eq!(
            &store[15 * 8..16 * 8],
            &[0xC5, 0x7E, 0x7F, 0xB8, 0x00, 0x05, 0, 0]
        );
        assert!(!store.contains(&0x62), "AVX-only boundary emitted EVEX");

        let mut load = X86_64Lowerer::new();
        load.set_avx_ymm16_vector_state(true);
        load.emit_helper_vector_state(PhysReg::Rcx, false);
        let load = load.code.as_slice();
        assert_eq!(load.len(), 16 * 8 + 7);
        assert_eq!(&load[..8], &[0xC5, 0xFE, 0x6F, 0x81, 0x40, 0x01, 0, 0]);
        assert_eq!(
            &load[15 * 8..16 * 8],
            &[0xC5, 0x7E, 0x6F, 0xB9, 0x00, 0x05, 0, 0]
        );
        assert!(!load.contains(&0x62), "AVX-only boundary emitted EVEX");
    }
}
