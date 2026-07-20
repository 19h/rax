//! System/privileged op execution

use crate::isa::x86_64::execute::system::{
    X86ControlWriteFault, X86ControlWriteState, X86MsrFault, X86MsrState, X86PmcFault, X86PmcState,
    read_x86_msr, read_x86_pmc, validate_x86_control_write, validate_x86_msr_write,
};
use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86ControlReg, X86CountKind, X86DebugReg, X86DescriptorTable,
    X86LmswSource, X86MonitorMwaitOp, X86OpHint, X86SmswTarget, X86ThreeDNowKind,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
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

            OpKind::X86Clts => {
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                };
                // Real-address mode permits CLTS regardless of stale CS.RPL.
                // X86RegState.cpl is the effective CPL, including VM86 as CPL3.
                if x86.cr0 & 1 != 0 && x86.cpl != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }
                x86.cr0 &= !(1 << 3);
            }

            OpKind::X86Msr(msr) => {
                let index = ctx.read_vreg(msr.ecx) as u32;
                let write_value = ((ctx.read_vreg(msr.edx) & u64::from(u32::MAX)) << 32)
                    | (ctx.read_vreg(msr.eax) & u64::from(u32::MAX));
                let tsc_base = ctx.cycle_count;
                let state = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => {
                        if x86.cr0 & 1 != 0 && x86.cpl != 0 {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                        X86MsrState {
                            cr0: x86.cr0,
                            tsc_adjust: x86.tsc_adjust,
                            tsc_aux: x86.tsc_aux,
                            efer: x86.efer,
                            star: x86.star,
                            lstar: x86.lstar,
                            cstar: x86.cstar,
                            fmask: x86.fmask,
                            sysenter_cs: x86.sysenter_cs,
                            sysenter_esp: x86.sysenter_esp,
                            sysenter_eip: x86.sysenter_eip,
                            fs_base: x86.fs_base,
                            gs_base: x86.gs_base,
                            kernel_gs_base: x86.kernel_gs_base,
                        }
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let tsc = tsc_base.wrapping_add(state.tsc_adjust);

                if msr.write {
                    let effect = match validate_x86_msr_write(index, write_value, state, tsc) {
                        Ok(effect) => effect,
                        Err(X86MsrFault::GeneralProtection) => {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                    };
                    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                        unreachable!("validated x86 MSR state changed")
                    };
                    x86.tsc_adjust = effect.state.tsc_adjust;
                    x86.tsc_aux = effect.state.tsc_aux;
                    x86.efer = effect.state.efer;
                    x86.star = effect.state.star;
                    x86.lstar = effect.state.lstar;
                    x86.cstar = effect.state.cstar;
                    x86.fmask = effect.state.fmask;
                    x86.sysenter_cs = effect.state.sysenter_cs;
                    x86.sysenter_esp = effect.state.sysenter_esp;
                    x86.sysenter_eip = effect.state.sysenter_eip;
                    x86.fs_base = effect.state.fs_base;
                    x86.gs_base = effect.state.gs_base;
                    x86.kernel_gs_base = effect.state.kernel_gs_base;
                } else {
                    let value = match read_x86_msr(index, state, tsc) {
                        Ok(value) => value,
                        Err(X86MsrFault::GeneralProtection) => {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                    };
                    Self::write_x86_partial(
                        ctx,
                        msr.eax,
                        value & u64::from(u32::MAX),
                        OpWidth::W32,
                    );
                    Self::write_x86_partial(ctx, msr.edx, value >> 32, OpWidth::W32);
                }
            }

            OpKind::X86ReadControl { dst, control } => {
                let value = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => {
                        // Real-address mode permits the read regardless of a
                        // stale CS.RPL. X86RegState.cpl already maps VM86 to
                        // effective CPL3.
                        if x86.cr0 & 1 != 0 && x86.cpl != 0 {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                        match control {
                            X86ControlReg::Cr0 => x86.cr0,
                            X86ControlReg::Cr2 => x86.cr2,
                            X86ControlReg::Cr3 => x86.cr3,
                            X86ControlReg::Cr4 => x86.cr4,
                            X86ControlReg::Cr8 => x86.cr8,
                        }
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                Self::write_x86_partial(ctx, *dst, value, OpWidth::W64);
            }

            OpKind::X86Smsw(smsw) => {
                let target_shape = match &smsw.target {
                    X86SmswTarget::Register { dst, width } => {
                        matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
                            && matches!(
                                dst,
                                VReg::Arch(ArchReg::X86(reg))
                                    if reg.gpr_index().is_some_and(|index| {
                                        index < 16 || smsw.requires_apx
                                    })
                            )
                    }
                    X86SmswTarget::Memory { addr } => {
                        let uses_egpr = addr.regs().iter().any(
                            |reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()),
                        );
                        addr.is_x86_state_backed_shape() && (!uses_egpr || smsw.requires_apx)
                    }
                };
                if !target_shape {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let cr0 = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => {
                        if smsw.requires_apx && !x86.apx_enabled {
                            ctx.request_exit(ExitReason::Undefined {
                                addr: op.guest_pc,
                                opcode: 0,
                            });
                            return Ok(());
                        }
                        // Real-address mode has effective CPL 0. In protected
                        // mode CR4.UMIP blocks SMSW above CPL 0 with #GP(0).
                        if x86.cr0 & 1 != 0 && x86.cr4 & (1 << 11) != 0 && x86.cpl != 0 {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                        x86.cr0
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };

                match &smsw.target {
                    X86SmswTarget::Register { dst, width } => {
                        Self::write_x86_partial(ctx, *dst, cr0, *width);
                    }
                    X86SmswTarget::Memory { addr } => {
                        let effective_addr = self.compute_address(ctx, addr);
                        self.store_memory(memory, effective_addr, cr0, MemWidth::B2)?;
                    }
                }
            }

            OpKind::X86Lmsw(lmsw) => {
                let instruction_len = lmsw.next_pc.checked_sub(op.guest_pc);
                let source_shape = match &lmsw.source {
                    X86LmswSource::Register { src } => matches!(
                        src,
                        VReg::Arch(ArchReg::X86(reg))
                            if reg.gpr_index().is_some_and(|index| {
                                index < 16 || lmsw.requires_apx
                            })
                    ),
                    X86LmswSource::Memory { addr } => {
                        let uses_egpr = addr.regs().iter().any(
                            |reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()),
                        );
                        addr.is_x86_state_backed_shape() && (!uses_egpr || lmsw.requires_apx)
                    }
                };
                if !matches!(instruction_len, Some(3..=15))
                    || op.x86_hint.is_some()
                    || !source_shape
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let old_cr0 = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => {
                        if lmsw.requires_apx && !x86.apx_enabled {
                            ctx.request_exit(ExitReason::Undefined {
                                addr: op.guest_pc,
                                opcode: 0,
                            });
                            return Ok(());
                        }
                        if x86.cr0 & 1 != 0 && x86.cpl != 0 {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                        x86.cr0
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };

                let source = match &lmsw.source {
                    X86LmswSource::Register { src } => ctx.read_vreg(*src),
                    X86LmswSource::Memory { addr } => {
                        let effective_addr = self.compute_address(ctx, addr);
                        self.load_memory(memory, effective_addr, MemWidth::B2, SignExtend::Zero)?
                    }
                };
                let new_cr0 = (old_cr0 & !0xF) | (source & 0xF) | (old_cr0 & 1);
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    unreachable!("validated x86 LMSW state changed")
                };
                x86.cr0 = new_cr0;
            }

            OpKind::X86DescriptorTableStore(store) => {
                let uses_egpr = store
                    .addr
                    .regs()
                    .iter()
                    .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
                if op.x86_hint.is_some()
                    || !store.addr.is_x86_state_backed_shape()
                    || (uses_egpr && !store.requires_apx)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let (limit, base) = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => {
                        if store.requires_apx && !x86.apx_enabled {
                            ctx.request_exit(ExitReason::Undefined {
                                addr: op.guest_pc,
                                opcode: 0,
                            });
                            return Ok(());
                        }
                        if x86.cr4 & (1 << 11) != 0 && x86.cpl != 0 {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                        match store.table {
                            X86DescriptorTable::Gdt => (x86.gdtr_limit, x86.gdtr_base),
                            X86DescriptorTable::Idt => (x86.idtr_limit, x86.idtr_base),
                        }
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };

                let effective_addr = self.compute_address(ctx, &store.addr);
                let mut payload = [0u8; 10];
                payload[..2].copy_from_slice(&limit.to_le_bytes());
                payload[2..].copy_from_slice(&base.to_le_bytes());
                memory.write(effective_addr, &payload)?;
            }

            OpKind::X86WriteControl {
                src,
                control,
                next_pc: _,
            } => {
                let value = ctx.read_vreg(*src);
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                };

                // Real-address mode permits the write. Effective CPL already
                // maps virtual-8086 execution to CPL3. All validation precedes
                // every architectural state update.
                if x86.cr0 & 1 != 0 && x86.cpl != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }
                let selector = match control {
                    X86ControlReg::Cr0 => 0,
                    X86ControlReg::Cr2 => 2,
                    X86ControlReg::Cr3 => 3,
                    X86ControlReg::Cr4 => 4,
                    X86ControlReg::Cr8 => 8,
                };
                let effect = match validate_x86_control_write(
                    selector,
                    value,
                    X86ControlWriteState {
                        cr0: x86.cr0,
                        cr3: x86.cr3,
                        cr4: x86.cr4,
                        efer: x86.efer,
                        cs_l: x86.cs_l,
                        tr_type: x86.tr_type,
                    },
                ) {
                    Ok(effect) => effect,
                    Err(X86ControlWriteFault::GeneralProtection) => {
                        ctx.request_exit(ExitReason::GeneralProtection {
                            addr: op.guest_pc,
                            error_code: 0,
                        });
                        return Ok(());
                    }
                };

                match control {
                    X86ControlReg::Cr0 => x86.cr0 = effect.value,
                    X86ControlReg::Cr2 => x86.cr2 = effect.value,
                    X86ControlReg::Cr3 => x86.cr3 = effect.value,
                    X86ControlReg::Cr4 => x86.cr4 = effect.value,
                    X86ControlReg::Cr8 => x86.cr8 = effect.value,
                }
                x86.efer = effect.efer;
            }

            OpKind::X86ReadDebug { dst, debug } => {
                const CR4_DE: u64 = 1 << 3;
                const DR6_BD: u64 = 1 << 13;
                const DR7_GD: u64 = 1 << 13;

                let value = match &mut ctx.arch_regs {
                    ArchRegState::X86_64(x86) => {
                        // General detect is a fault before the MOV executes.
                        // Set BD before reporting it; GD is cleared only when
                        // the architectural #DB handler is actually entered.
                        if x86.dr7 & DR7_GD != 0 {
                            x86.dr6 |= DR6_BD;
                            ctx.request_exit(ExitReason::Debug { addr: op.guest_pc });
                            return Ok(());
                        }
                        if matches!(debug, X86DebugReg::Dr4 | X86DebugReg::Dr5)
                            && x86.cr4 & CR4_DE != 0
                        {
                            ctx.request_exit(ExitReason::Undefined {
                                addr: op.guest_pc,
                                opcode: 0,
                            });
                            return Ok(());
                        }
                        // Real-address mode permits the read. Effective CPL
                        // already maps virtual-8086 execution to CPL3.
                        if x86.cr0 & 1 != 0 && x86.cpl != 0 {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                        match debug {
                            X86DebugReg::Dr0 => x86.dr0,
                            X86DebugReg::Dr1 => x86.dr1,
                            X86DebugReg::Dr2 => x86.dr2,
                            X86DebugReg::Dr3 => x86.dr3,
                            X86DebugReg::Dr4 | X86DebugReg::Dr6 => x86.dr6,
                            X86DebugReg::Dr5 | X86DebugReg::Dr7 => x86.dr7,
                        }
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                Self::write_x86_partial(ctx, *dst, value, OpWidth::W64);
            }

            OpKind::X86WriteDebug { src, debug } => {
                const CR4_DE: u64 = 1 << 3;
                const DR6_BD: u64 = 1 << 13;
                const DR7_GD: u64 = 1 << 13;

                let value = ctx.read_vreg(*src);
                match &mut ctx.arch_regs {
                    ArchRegState::X86_64(x86) => {
                        // Match direct execution priority. The write is wholly
                        // non-committing on every fault path.
                        if x86.dr7 & DR7_GD != 0 {
                            x86.dr6 |= DR6_BD;
                            ctx.request_exit(ExitReason::Debug { addr: op.guest_pc });
                            return Ok(());
                        }
                        if matches!(debug, X86DebugReg::Dr4 | X86DebugReg::Dr5)
                            && x86.cr4 & CR4_DE != 0
                        {
                            ctx.request_exit(ExitReason::Undefined {
                                addr: op.guest_pc,
                                opcode: 0,
                            });
                            return Ok(());
                        }
                        if x86.cr0 & 1 != 0 && x86.cpl != 0 {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                        if matches!(
                            debug,
                            X86DebugReg::Dr4
                                | X86DebugReg::Dr5
                                | X86DebugReg::Dr6
                                | X86DebugReg::Dr7
                        ) && value >> 32 != 0
                        {
                            ctx.request_exit(ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            });
                            return Ok(());
                        }
                        match debug {
                            X86DebugReg::Dr0 => x86.dr0 = value,
                            X86DebugReg::Dr1 => x86.dr1 = value,
                            X86DebugReg::Dr2 => x86.dr2 = value,
                            X86DebugReg::Dr3 => x86.dr3 = value,
                            X86DebugReg::Dr4 | X86DebugReg::Dr6 => x86.dr6 = value,
                            X86DebugReg::Dr5 | X86DebugReg::Dr7 => x86.dr7 = value,
                        }
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                }
            }

            OpKind::X86ReadTsc(read) => {
                let (cr0, cr4, cpl, tsc_aux) = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => (x86.cr0, x86.cr4, x86.cpl, x86.tsc_aux),
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if cr0 & 1 != 0 && cr4 & (1 << 2) != 0 && cpl != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }
                let tsc = ctx.cycle_count;
                Self::write_x86_partial(ctx, read.dst_lo, tsc & u32::MAX as u64, OpWidth::W32);
                Self::write_x86_partial(ctx, read.dst_hi, tsc >> 32, OpWidth::W32);
                if let Some(dst_aux) = read.dst_aux {
                    Self::write_x86_partial(ctx, dst_aux, u64::from(tsc_aux), OpWidth::W32);
                }
            }

            OpKind::X86ReadPmc(read) => {
                let selector = ctx.read_vreg(read.selector) as u32;
                let state = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => X86PmcState {
                        cr0: x86.cr0,
                        cr4: x86.cr4,
                        cpl: x86.cpl,
                    },
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let value = match read_x86_pmc(selector, state, ctx.cycle_count) {
                    Ok(value) => value,
                    Err(X86PmcFault::GeneralProtection) => {
                        ctx.request_exit(ExitReason::GeneralProtection {
                            addr: op.guest_pc,
                            error_code: 0,
                        });
                        return Ok(());
                    }
                };
                Self::write_x86_partial(
                    ctx,
                    read.dst_lo,
                    value & u64::from(u32::MAX),
                    OpWidth::W32,
                );
                Self::write_x86_partial(ctx, read.dst_hi, value >> 32, OpWidth::W32);
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

            OpKind::X86FsGsBase {
                operand,
                base,
                write,
                width,
                requires_apx,
            } => {
                let (cr4, apx_enabled) = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => (x86.cr4, x86.apx_enabled),
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if cr4 & (1 << 16) == 0 || (*requires_apx && !apx_enabled) {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                if *write {
                    let value = ctx.read_vreg(*operand) & width.mask();
                    if *width == OpWidth::W64 && (((value as i64) << 16 >> 16) as u64 != value) {
                        ctx.request_exit(ExitReason::GeneralProtection {
                            addr: op.guest_pc,
                            error_code: 0,
                        });
                        return Ok(());
                    }
                    ctx.write_vreg(*base, value);
                } else {
                    let value = ctx.read_vreg(*base);
                    Self::write_x86_partial(ctx, *operand, value, *width);
                }
            }

            OpKind::X86SwapGs {
                gs_base,
                kernel_gs_base,
            } => {
                let cpl = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.cpl,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if cpl != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }
                let old_gs_base = ctx.read_vreg(*gs_base);
                let old_kernel_gs_base = ctx.read_vreg(*kernel_gs_base);
                ctx.write_vreg(*gs_base, old_kernel_gs_base);
                ctx.write_vreg(*kernel_gs_base, old_gs_base);
            }

            OpKind::X86MonitorMwait(X86MonitorMwaitOp {
                rcx,
                hint,
                addr,
                stack_segment,
            }) => {
                let cpl = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.cpl,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if cpl != 0 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                if ctx.read_vreg(*rcx) != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }
                // EDX for MONITOR and EAX for MWAIT are architecturally read
                // hint inputs. Their values are implementation-dependent and
                // intentionally have no effect in the deterministic profile.
                let _ = ctx.read_vreg(*hint);
                if let Some(addr) = addr {
                    let effective_addr = self.compute_address(ctx, addr);
                    if (((effective_addr as i64) << 16 >> 16) as u64) != effective_addr {
                        ctx.request_exit(if *stack_segment {
                            ExitReason::StackSegment {
                                addr: op.guest_pc,
                                error_code: 0,
                            }
                        } else {
                            ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            }
                        });
                        return Ok(());
                    }
                    let _ =
                        self.load_memory(memory, effective_addr, MemWidth::B1, SignExtend::Zero)?;
                }
            }

            OpKind::X86Pkru {
                eax,
                ecx,
                edx,
                pkru,
                write,
            } => {
                let cr4 = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.cr4,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if cr4 & (1 << 22) == 0 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let ecx_value = ctx.read_vreg(*ecx) as u32;
                if ecx_value != 0 || (*write && ctx.read_vreg(*edx) as u32 != 0) {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }

                if *write {
                    ctx.write_vreg(*pkru, ctx.read_vreg(*eax) as u32 as u64);
                } else {
                    let value = ctx.read_vreg(*pkru) as u32 as u64;
                    Self::write_x86_partial(ctx, *eax, value, OpWidth::W32);
                    Self::write_x86_partial(ctx, *edx, 0, OpWidth::W32);
                }
            }

            _ => return self.execute_op_meta(ctx, memory, op),
        }

        Ok(())
    }
}
