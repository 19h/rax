//! System/privileged op execution

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86CountKind, X86OpHint, X86ThreeDNowKind, X86X87ArithmeticDestination,
    X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant, X86X87ControlKind, X86X87DataKind,
    X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth, X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};
use std::cmp::Ordering;
use std::collections::HashMap;

impl SmirInterpreter {
    pub(crate) fn execute_op_system(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // SYSTEM / PRIVILEGED
            // ==================================================================
            OpKind::Syscall { num, args } => {
                let num_val = ctx.read_vreg(*num);
                let arg_vals: Vec<u64> = args.iter().map(|a| ctx.read_vreg(*a)).collect();
                ctx.request_exit(ExitReason::Syscall {
                    num: num_val,
                    args: arg_vals,
                });
            }

            OpKind::Swi { imm } => {
                ctx.request_exit(ExitReason::Syscall {
                    num: *imm as u64,
                    args: vec![],
                });
            }

            OpKind::ReadSysReg { dst, reg: _ } => {
                // Simplified: return 0
                ctx.write_vreg(*dst, 0);
            }

            OpKind::WriteSysReg { reg: _, src: _ } => {
                // Simplified: no-op
            }

            OpKind::X86ReadTsc { dst_lo, dst_hi } => {
                let tsc = ctx.cycle_count;
                Self::write_x86_partial(ctx, *dst_lo, tsc & u32::MAX as u64, OpWidth::W32);
                Self::write_x86_partial(ctx, *dst_hi, tsc >> 32, OpWidth::W32);
            }

            OpKind::X86Cpuid {
                dst_eax,
                dst_ebx,
                dst_ecx,
                dst_edx,
                leaf,
                subleaf,
            } => {
                let leaf = ctx.read_vreg(*leaf) as u32;
                let subleaf = ctx.read_vreg(*subleaf) as u32;
                let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                };
                let (eax, ebx, ecx, edx) = crate::isa::x86_64::execute::system::evaluate_cpuid(
                    leaf,
                    subleaf,
                    crate::isa::x86_64::execute::system::X86CpuidState {
                        cr4: x86.cr4,
                        xcr0: x86.xcr0,
                        xeon_phi_avx512: x86.xeon_phi_avx512,
                        vp2intersect: x86.vp2intersect,
                        sse4a: x86.sse4a,
                        apx: x86.apx_enabled,
                    },
                );
                for (dst, value) in [
                    (*dst_eax, eax),
                    (*dst_ebx, ebx),
                    (*dst_ecx, ecx),
                    (*dst_edx, edx),
                ] {
                    Self::write_x86_partial(ctx, dst, u64::from(value), OpWidth::W32);
                }
            }

            _ => return self.execute_op_meta(ctx, memory, op),
        }

        Ok(())
    }
}
