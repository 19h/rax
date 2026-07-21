//! Legacy system-segment selector stores and loads.

use crate::smir::ir::TrapKind;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource,
    X86SystemSelectorStoreOp, X86SystemSelectorTarget,
};
use crate::smir::ir::types::{MemWidth, OpId};
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix, decode_modrm};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    /// Lift long-mode `PUSH FS` (`0F A0`) and `PUSH GS` (`0F A8`). The stack
    /// address is always 64 bits. The operand is 8 bytes by default, 2 bytes
    /// under 66H, and 8 bytes when REX.W/REX2.W overrides 66H. A single stack
    /// target preserves the architectural non-commit rule when the write
    /// faults. REX2 availability remains a dynamic APX check.
    pub(crate) fn lift_push_segment_0f(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![opcode],
            });
        }

        let selector = match opcode {
            0xA0 => X86SystemSelector::Fs,
            0xA8 => X86SystemSelector::Gs,
            _ => unreachable!("PUSH-segment dispatcher admitted another opcode"),
        };
        let width = if prefix.operand_size_override && !prefix.rex_w() {
            MemWidth::B2
        } else {
            MemWidth::B8
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
                    selector,
                    target: X86SystemSelectorTarget::Stack {
                        stack_pointer: self.rsp(),
                        width,
                    },
                    requires_apx: prefix.rex2.is_some(),
                }),
            )],
            prefix.cursor,
        ))
    }

    /// Lift long-mode `POP FS` (`0F A1`) and `POP GS` (`0F A9`). The stack
    /// address is always 64 bits. The operand is 8 bytes by default, 2 bytes
    /// under 66H, and 8 bytes when REX.W/REX2.W overrides 66H. Selector and
    /// hidden-cache state plus RSP commit only after the complete stack read
    /// and descriptor transition succeed. REX2 availability remains dynamic.
    pub(crate) fn lift_pop_segment_0f(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![opcode],
            });
        }

        let selector = match opcode {
            0xA1 => X86SystemSelector::Fs,
            0xA9 => X86SystemSelector::Gs,
            _ => unreachable!("POP-segment dispatcher admitted another opcode"),
        };
        let width = if prefix.operand_size_override && !prefix.rex_w() {
            MemWidth::B2
        } else {
            MemWidth::B8
        };
        let next_pc = pc.wrapping_add(prefix.cursor as u64);

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
                    selector,
                    source: X86SystemSelectorSource::Stack {
                        stack_pointer: self.rsp(),
                        width,
                    },
                    requires_apx: prefix.rex2.is_some(),
                    next_pc,
                }),
            )],
            prefix.cursor,
        ))
    }

    /// Lift `MOV r/m16/32/64, Sreg` (`8C /r`). The ModR/M.reg field selects
    /// ES/CS/SS/DS/FS/GS and ignores both legacy REX.R and REX2.R4/R3. Register
    /// destinations use the encoded operand width; memory destinations always
    /// store exactly 2 bytes. REX2 availability remains a dynamic APX check.
    pub(crate) fn lift_segment_selector_store_8c(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(1)].to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let selector = match (modrm.byte >> 3) & 7 {
            0 => X86SystemSelector::Es,
            1 => X86SystemSelector::Cs,
            2 => X86SystemSelector::Ss,
            3 => X86SystemSelector::Ds,
            4 => X86SystemSelector::Fs,
            5 => X86SystemSelector::Gs,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes[..bytes.len().min(1)].to_vec(),
                });
            }
        };
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let target = if let Some(x86_addr) = modrm.addr.as_ref() {
            let next_pc = pc.wrapping_add(bytes_consumed as u64);
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            debug_assert!(pre_ops.is_empty());
            X86SystemSelectorTarget::Memory { addr }
        } else {
            X86SystemSelectorTarget::Register {
                dst: self.gpr(modrm.rm),
                width: prefix.op_width(),
            }
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
                    selector,
                    target,
                    requires_apx: prefix.rex2.is_some(),
                }),
            )],
            bytes_consumed,
        ))
    }

    /// Lift SLDT/STR (`0F 00 /0` and `/1`). Register destinations follow the
    /// encoded 16-/32-/64-bit operand width; memory destinations are fixed at
    /// 2 bytes. Protected-mode, APX, and UMIP checks remain dynamic.
    pub(crate) fn lift_system_selector_store_0f00(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(1)].to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let selector = match (modrm.byte >> 3) & 7 {
            0 => X86SystemSelector::Ldtr,
            1 => X86SystemSelector::Tr,
            _ => unreachable!("0F 00 selector-store dispatcher admitted another group"),
        };
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let target = if let Some(x86_addr) = modrm.addr.as_ref() {
            let next_pc = pc.wrapping_add(bytes_consumed as u64);
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            debug_assert!(pre_ops.is_empty());
            X86SystemSelectorTarget::Memory { addr }
        } else {
            X86SystemSelectorTarget::Register {
                dst: self.gpr(modrm.rm),
                width: prefix.op_width(),
            }
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
                    selector,
                    target,
                    requires_apx: prefix.rex2.is_some(),
                }),
            )],
            bytes_consumed,
        ))
    }

    /// Lift LLDT/LTR (`0F 00 /2` and `/3`). Both register and memory sources
    /// are fixed at 16 bits; operand-size prefixes are ignored. APX
    /// availability, execution mode, privilege, descriptor validation, the
    /// LTR busy transition, and serialization remain dynamic properties.
    pub(crate) fn lift_system_selector_load_0f00(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(1)].to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let selector = match (modrm.byte >> 3) & 7 {
            2 => X86SystemSelector::Ldtr,
            3 => X86SystemSelector::Tr,
            _ => unreachable!("0F 00 selector-load dispatcher admitted another group"),
        };
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let source = if let Some(x86_addr) = modrm.addr.as_ref() {
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            debug_assert!(pre_ops.is_empty());
            let stack_segment = match prefix.segment_override {
                Some(0x36) => true,
                Some(_) => false,
                None => x86_addr.base.is_some_and(|base| matches!(base & 7, 4 | 5)),
            };
            X86SystemSelectorSource::Memory {
                addr,
                width: MemWidth::B2,
                stack_segment,
            }
        } else {
            X86SystemSelectorSource::Register {
                src: self.gpr(modrm.rm),
            }
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
                    selector,
                    source,
                    requires_apx: prefix.rex2.is_some(),
                    next_pc,
                }),
            )],
            bytes_consumed,
        ))
    }

    /// Lift `MOV Sreg,r/m` (`8E /r`). ModR/M.reg selects ES/SS/DS/FS/GS and
    /// ignores REX.R plus both REX2 R extension bits. Register sources always
    /// contribute their low 16 bits. Memory sources are 2 bytes unless W=1
    /// selects the Intel-defined 8-byte read whose low 16 bits are loaded.
    pub(crate) fn lift_segment_selector_load_8e(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(1)].to_vec(),
            });
        }

        let Some(&raw_modrm) = bytes.first() else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor,
                need: prefix.cursor + 1,
            });
        };
        let selector = match (raw_modrm >> 3) & 7 {
            0 => X86SystemSelector::Es,
            2 => X86SystemSelector::Ss,
            3 => X86SystemSelector::Ds,
            4 => X86SystemSelector::Fs,
            5 => X86SystemSelector::Gs,
            // CS and /6-/7 are invalid independently of the source operand.
            1 | 6 | 7 => {
                return Ok(LiftResult {
                    ops: vec![],
                    bytes_consumed: prefix.cursor + 1,
                    control_flow: ControlFlow::Trap {
                        kind: TrapKind::InvalidOpcode,
                    },
                    branch_targets: vec![],
                });
            }
            _ => unreachable!("three-bit segment selector changed"),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let source = if let Some(x86_addr) = modrm.addr.as_ref() {
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            debug_assert!(pre_ops.is_empty());
            let stack_segment = match prefix.segment_override {
                Some(0x36) => true,
                Some(_) => false,
                None => x86_addr.base.is_some_and(|base| matches!(base & 7, 4 | 5)),
            };
            X86SystemSelectorSource::Memory {
                addr,
                width: if prefix.rex_w() {
                    MemWidth::B8
                } else {
                    MemWidth::B2
                },
                stack_segment,
            }
        } else {
            X86SystemSelectorSource::Register {
                src: self.gpr(modrm.rm),
            }
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
                    selector,
                    source,
                    requires_apx: prefix.rex2.is_some(),
                    next_pc,
                }),
            )],
            bytes_consumed,
        ))
    }
}
