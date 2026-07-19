//! Transactional Synchronization Extensions fixed-encoding lifting.

use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    /// Lift XGETBV/XSETBV, RDPKRU/WRPKRU, SERIALIZE, SWAPGS, and the RTM fixed
    /// ModR/M encodings in 0F 01.
    pub(crate) fn lift_xcr_0f01(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        let Some(&modrm) = bytes.first() else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor,
                need: prefix.cursor + 1,
            });
        };
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..1].to_vec(),
            });
        }

        let kind = match modrm {
            0xD0 | 0xD1 if prefix.rep_prefix.is_none() && !prefix.operand_size_override => {
                if modrm == 0xD0 {
                    OpKind::X86XGetBv {
                        dst_low: self.gpr(0),
                        dst_high: self.gpr(2),
                        selector: self.gpr(1),
                    }
                } else {
                    OpKind::X86XSetBv {
                        selector: self.gpr(1),
                        src_low: self.gpr(0),
                        src_high: self.gpr(2),
                    }
                }
            }
            0xD0 | 0xD1 => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes[..1].to_vec(),
                });
            }
            0xD6 => OpKind::X86XTest,
            0xD5 => {
                // The SMIR RTM profile deterministically forces XBEGIN down
                // its abort path and therefore never enters transactional
                // execution. XEND is consequently always outside an RTM
                // region and raises #GP(0).
                return Ok(LiftResult {
                    ops: Vec::new(),
                    bytes_consumed: prefix.cursor + 1,
                    control_flow: ControlFlow::Trap {
                        kind: TrapKind::GeneralProtection,
                    },
                    branch_targets: Vec::new(),
                });
            }
            0xEE | 0xEF if prefix.rep_prefix == Some(0xF3) => {
                // F3 0F 01 EE/EF select CLUI/STUI, not PKRU. The emulator's
                // profile does not expose User Interrupts, so both aliases
                // deterministically raise #UD rather than entering fallback.
                return Ok(LiftResult {
                    ops: Vec::new(),
                    bytes_consumed: prefix.cursor + 1,
                    control_flow: ControlFlow::Trap {
                        kind: TrapKind::InvalidOpcode,
                    },
                    branch_targets: Vec::new(),
                });
            }
            0xE8 if prefix.rep_prefix == Some(0xF2) => {
                // F2 0F 01 E8 selects XSUSLDTRK. The guest profile does not
                // expose TSX load-address tracking, so the alias is #UD.
                return Ok(LiftResult {
                    ops: Vec::new(),
                    bytes_consumed: prefix.cursor + 1,
                    control_flow: ControlFlow::Trap {
                        kind: TrapKind::InvalidOpcode,
                    },
                    branch_targets: Vec::new(),
                });
            }
            0xE8 if prefix.rep_prefix == Some(0xF3) => {
                // F3 0F 01 E8 selects CET SETSSBSY. CET shadow stacks are not
                // exposed by this guest profile, so the alias is #UD.
                return Ok(LiftResult {
                    ops: Vec::new(),
                    bytes_consumed: prefix.cursor + 1,
                    control_flow: ControlFlow::Trap {
                        kind: TrapKind::InvalidOpcode,
                    },
                    branch_targets: Vec::new(),
                });
            }
            0xE8 => OpKind::Fence {
                kind: FenceKind::InstructionSerialize,
            },
            0xEE | 0xEF => OpKind::X86Pkru {
                eax: self.gpr(0),
                ecx: self.gpr(1),
                edx: self.gpr(2),
                pkru: VReg::Arch(ArchReg::X86(X86Reg::Pkru)),
                write: modrm == 0xEF,
            },
            0xF8 => OpKind::X86SwapGs {
                gs_base: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                kernel_gs_base: VReg::Arch(ArchReg::X86(X86Reg::KernelGsBase)),
            },
            _ => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("0F 01 {modrm:02X}"),
                });
            }
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(OpId(0), pc, kind)],
            prefix.cursor + 1,
        ))
    }
}
