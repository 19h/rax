//! Flag-only AArch64 scalar operations on an x86-64 host.

use crate::smir::ir::types::{OpWidth, SrcOperand, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::x86_64::X86Emitter;

use super::{ACC, Aarch64X86_64Lowerer, FlagForm, RHS};

impl Aarch64X86_64Lowerer {
    pub(super) fn lower_flag_only_comparison(
        &mut self,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        form: FlagForm,
    ) -> Result<(), LowerError> {
        self.load_vreg_to(src1, ACC, width)?;
        self.load_src_to(src2, RHS, width)?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            match form {
                FlagForm::Add => emitter.emit_add_rr(ACC, RHS, width),
                FlagForm::Sub => emitter.emit_cmp_rr(ACC, RHS, width),
                FlagForm::Logic => emitter.emit_test_rr(ACC, RHS, width),
            }
        }
        self.emit_nzcv_from_flags(form);
        Ok(())
    }
}
