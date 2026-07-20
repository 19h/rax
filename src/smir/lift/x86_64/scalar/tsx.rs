//! Fixed-encoding 0F 01 system and transactional instruction lifting.

use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    /// Lift VMCALL/VMMCALL hints, disabled VMX controls, MONITOR/MWAIT,
    /// CLAC/STAC, XGETBV/XSETBV, RDPKRU/WRPKRU, SERIALIZE, SWAPGS, RDTSCP,
    /// and the RTM fixed ModR/M encodings in 0F 01.
    pub(crate) fn lift_xcr_0f01(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
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

        if modrm == 0xD9 && (prefix.rep_prefix.is_some() || prefix.rex2.is_some()) {
            // AMD assigns F2/F3 0F 01 D9 to VMGEXIT. The configured guest
            // profile exposes neither SVM nor SEV-ES, so those aliases are
            // #UD. REX2 is Intel APX while VMMCALL is AMD-only; consequently
            // the compressed D9 encoding is undefined on both vendor profiles.
            return Ok(LiftResult {
                ops: Vec::new(),
                bytes_consumed: prefix.cursor + 1,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: Vec::new(),
            });
        }

        if matches!(modrm, 0xC1 | 0xD9) && prefix.rex2.is_none() {
            // RAX's deterministic non-virtualized profile treats ordinary
            // VMCALL/VMMCALL as paravirtualized hints: no register, flag,
            // memory, or control-state effect beyond instruction advance.
            return Ok(LiftResult::fallthrough(Vec::new(), prefix.cursor + 1));
        }

        if matches!(modrm, 0xC2 | 0xC3 | 0xC4 | 0xD4) {
            // VMLAUNCH, VMRESUME, and VMXOFF first raise #UD outside VMX
            // operation; VMFUNC raises #UD outside VMX non-root operation.
            // RAX exposes neither execution state, so these four controls are
            // deterministic fault-class traps before CPL, VMCS, EAX, flags,
            // or any other architectural state can be observed or committed.
            return Ok(LiftResult {
                ops: Vec::new(),
                bytes_consumed: prefix.cursor + 1,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: Vec::new(),
            });
        }

        if matches!(modrm, 0xC8 | 0xC9) {
            let addr = if modrm == 0xC8 {
                let x86_addr = X86Address {
                    base: Some(0),
                    index: None,
                    scale: 1,
                    disp: 0,
                    rip_relative: false,
                    address_width: if prefix.address_size_override {
                        OpWidth::W32
                    } else {
                        OpWidth::W64
                    },
                    disp_size: DispSize::Auto,
                    segment: match prefix.segment_override {
                        Some(0x64) => Some(X86Reg::FsBase),
                        Some(0x65) => Some(X86Reg::GsBase),
                        _ => None,
                    },
                };
                let next_rip = pc + prefix.cursor as u64 + 1;
                if prefix.address_size_override {
                    Some(Address::X86Addr32(Box::new(
                        self.x86_addr32_state_address(&x86_addr, next_rip),
                    )))
                } else {
                    let (addr, pre_ops) = self.x86_addr_to_smir(&x86_addr, next_rip, ctx);
                    debug_assert!(pre_ops.is_empty());
                    Some(addr)
                }
            } else {
                None
            };
            return Ok(LiftResult::fallthrough(
                vec![SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::X86MonitorMwait(X86MonitorMwaitOp {
                        rcx: self.gpr(1),
                        hint: self.gpr(if modrm == 0xC8 { 2 } else { 0 }),
                        addr,
                        stack_segment: modrm == 0xC8 && prefix.segment_override == Some(0x36),
                    }),
                )],
                prefix.cursor + 1,
            ));
        }

        let kind = match modrm {
            0xCA | 0xCB => OpKind::SetAC {
                value: modrm == 0xCB,
            },
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
            0xF9 => OpKind::X86ReadTsc(X86ReadTscOp {
                dst_lo: self.gpr(0),
                dst_hi: self.gpr(2),
                dst_aux: Some(self.gpr(1)),
            }),
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
