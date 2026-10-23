//! Arm StandardFPSCRValue for legacy Advanced SIMD arithmetic.
use super::*;

impl<'a, M: ArmMemory> Executor<'a, M> {
    pub(super) fn with_standard_fp_controls(
        &mut self,
        execute: impl FnOnce(&mut Self) -> ExecResult,
    ) -> ExecResult {
        let original = self.cpu.vfp.fpscr.bits();
        // StandardFPSCRValue preserves AHP/FZ16, selects DN/FZ, and clears
        // rounding and exception enables. Only cumulative exceptions merge back.
        let standard = (original & ((1 << 26) | (1 << 19))) | (3 << 24);
        self.cpu.vfp.fpscr = Fpscr::from_bits(standard);
        let result = execute(self);
        let raised = self.cpu.vfp.fpscr.bits() & 0x9f;
        self.cpu.vfp.fpscr = Fpscr::from_bits(original | raised);
        result
    }
}
