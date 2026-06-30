//! Two-byte opcode instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::super::aes;
use super::super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::super::flags;
use super::super::super::super::insn;

impl X86_64Vcpu {
    #[inline(always)]
    pub(in crate::backend::emulator::x86_64) fn execute_0f01(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        // Peek at modrm to determine instruction
        let modrm = ctx.peek_u8()?;

        // Check for special instructions with mod=3
        if modrm >> 6 == 3 {
            match modrm {
                0xC1 => {
                    // VMCALL (0x0F 0x01 0xC1) - VMX hypercall
                    ctx.consume_u8()?; // consume modrm
                                       // In a real hypervisor, this would cause a VM exit.
                                       // When running without VMX, this should generate #UD.
                                       // For our emulator, treat as NOP - kernel uses this for
                                       // paravirtualized hints in delay loops.
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xC8 => {
                    // MONITOR (0x0F 0x01 0xC8) - Set up address range monitoring
                    ctx.consume_u8()?; // consume modrm
                                       // MONITOR sets up an address range for monitoring using RAX/EAX
                                       // For emulation, treat as NOP - no actual hardware monitoring
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xC9 => {
                    // MWAIT (0x0F 0x01 0xC9) - Monitor wait
                    ctx.consume_u8()?; // consume modrm
                                       // MWAIT hints processor to enter optimized state while waiting
                                       // For emulation, treat as NOP - no power management
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD0 => {
                    // XGETBV (0F 01 D0) - read extended control register XCR[ECX].
                    ctx.consume_u8()?; // consume modrm
                                       // Lenient on CR4.OSXSAVE since the harness reads XCRs directly.
                    let value = match self.regs.rcx as u32 {
                        0 => self.xcr0,
                        1 => self.xgetbv1_value,
                        _ => {
                            self.inject_exception(13, Some(0))?;
                            return Ok(None);
                        }
                    };
                    self.regs.rax = value & 0xFFFF_FFFF;
                    self.regs.rdx = (value >> 32) & 0xFFFF_FFFF;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD1 => {
                    // XSETBV (0F 01 D1) - write XCR[ECX] from EDX:EAX (privileged).
                    ctx.consume_u8()?; // consume modrm
                                       // #GP(0) if CPL != 0.
                    if self.sregs.cr0 & 1 != 0 && (self.sregs.cs.selector & 3) != 0 {
                        self.inject_exception(13, Some(0))?;
                        return Ok(None);
                    }
                    let ecx = self.regs.rcx as u32;
                    let value = (self.regs.rax & 0xFFFF_FFFF) | (self.regs.rdx << 32);
                    // Only XCR0 exists; x87 (bit0) must stay set; AVX (bit2) requires
                    // SSE (bit1); AVX-512 state bits must be enabled as a group;
                    // APX_F (bit19) enables APX EGPR state.
                    const XCR0_AVX512: u64 = (1 << 5) | (1 << 6) | (1 << 7);
                    const XCR0_APX_F: u64 = 1 << 19;
                    let supported =
                        0x7 | XCR0_AVX512 | if self.apx_enabled() { XCR0_APX_F } else { 0 };
                    let avx512_bits = value & XCR0_AVX512;
                    let invalid = ecx != 0
                        || (value & 1) == 0
                        || (value & !supported) != 0
                        || ((value & 0x4) != 0 && (value & 0x2) == 0)
                        || (avx512_bits != 0
                            && (avx512_bits != XCR0_AVX512 || (value & 0x6) != 0x6));
                    if invalid {
                        self.inject_exception(13, Some(0))?;
                        return Ok(None);
                    }
                    self.xcr0 = value;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD4 => {
                    // VMFUNC (0x0F 0x01 0xD4) requires VMX operation. The
                    // emulator does not expose VMX execution state, so this
                    // form is #UD.
                    ctx.consume_u8()?; // consume modrm
                    self.inject_undefined_instruction()
                }
                0xD5 => {
                    // XEND outside an active transaction raises #GP(0). The
                    // emulator has no transactional state, so every XEND is
                    // outside a transaction.
                    ctx.consume_u8()?; // consume modrm
                    self.inject_exception(13, Some(0))?;
                    Ok(None)
                }
                0xD9 => {
                    // VMMCALL (0x0F 0x01 0xD9) - AMD SVM hypercall
                    ctx.consume_u8()?; // consume modrm
                                       // Treat as NOP like VMCALL
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD6 => {
                    // XTEST sets ZF=1 when not in transactional execution. The
                    // emulator has no transactional state, so it is never in one.
                    ctx.consume_u8()?; // consume modrm
                    self.clear_lazy_flags();
                    self.regs.rflags |= flags::bits::ZF;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xCA => {
                    // CLAC (0x0F 0x01 0xCA) - Clear AC flag
                    ctx.consume_u8()?; // consume modrm
                                       // Note: AC is not a lazy flag, but clear for consistency
                                       // Materialize (don't discard) pending lazy flags - CLAC must
                                       // only clear AC, leaving ZF/SF/CF/etc. from prior ops intact.
                    self.materialize_flags();
                    self.regs.rflags &= !flags::bits::AC;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xCB => {
                    // STAC (0x0F 0x01 0xCB) - Set AC flag
                    ctx.consume_u8()?; // consume modrm
                                       // Note: AC is not a lazy flag, but clear for consistency
                                       // Materialize (don't discard) pending lazy flags - STAC must
                                       // only set AC, leaving ZF/SF/CF/etc. from prior ops intact.
                    self.materialize_flags();
                    self.regs.rflags |= flags::bits::AC;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xE8 if ctx.rep_prefix == Some(0xF3) => {
                    // SETSSBSY is a CET shadow-stack instruction. The emulator
                    // does not expose CET shadow stacks, so this form is #UD.
                    ctx.consume_u8()?; // consume modrm
                    self.inject_undefined_instruction()
                }
                0xE8 => {
                    // SERIALIZE (0x0F 0x01 0xE8) - Serialize instruction execution
                    ctx.consume_u8()?; // consume modrm
                                       // Serializing instruction - no architectural state changes in emulation.
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xEA if ctx.rep_prefix == Some(0xF3) => {
                    // SAVEPREVSSP is a CET shadow-stack instruction. The emulator
                    // does not expose CET shadow stacks, so this form is #UD.
                    ctx.consume_u8()?; // consume modrm
                    self.inject_undefined_instruction()
                }
                0xEC..=0xEF if ctx.rep_prefix == Some(0xF3) => {
                    // UIRET/TESTUI/CLUI/STUI require User Interrupts, which the
                    // emulator does not expose. Without this prefix-sensitive
                    // guard, CLUI/STUI aliases would be decoded as RDPKRU/WRPKRU.
                    ctx.consume_u8()?; // consume modrm
                    self.inject_undefined_instruction()
                }
                0xEE => {
                    // RDPKRU (0x0F 0x01 0xEE) - Read PKRU into EAX, clear EDX
                    ctx.consume_u8()?; // consume modrm
                    if (self.regs.rcx as u32) != 0 {
                        self.inject_exception(13, Some(0))?;
                        return Ok(None);
                    }
                    self.regs.rax = self.pkru as u64;
                    self.regs.rdx = 0;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xEF => {
                    // WRPKRU (0x0F 0x01 0xEF) - Write EAX into PKRU
                    ctx.consume_u8()?; // consume modrm
                    if (self.regs.rcx as u32) != 0 || (self.regs.rdx as u32) != 0 {
                        self.inject_exception(13, Some(0))?;
                        return Ok(None);
                    }
                    self.pkru = self.regs.rax as u32;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xF8 => {
                    // SWAPGS (0x0F 0x01 0xF8) - privileged: #GP(0) at CPL != 0
                    ctx.consume_u8()?; // consume modrm
                    if self.sregs.cr0 & 1 != 0 && (self.sregs.cs.selector & 3) != 0 {
                        self.inject_exception(13, Some(0))?;
                        return Ok(None);
                    }
                    // Exchange GS.base with IA32_KERNEL_GS_BASE MSR (0xC0000102)
                    let gs_base = self.sregs.gs.base;
                    let kernel_gs_base = self.kernel_gs_base;
                    self.sregs.gs.base = kernel_gs_base;
                    self.kernel_gs_base = gs_base;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xF9 => {
                    // RDTSCP (0x0F 0x01 0xF9)
                    ctx.consume_u8()?; // consume modrm
                    insn::system::rdtscp(self, ctx)
                }
                _ => insn::system::group7(self, ctx),
            }
        } else {
            insn::system::group7(self, ctx)
        }
    }

    /// Execute 0x0F 0xAE opcodes (Group 15 - fences, CLFLUSH, etc.)
    #[inline(always)]
    pub(in crate::backend::emulator::x86_64) fn execute_0fae(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.consume_u8()?;
        let reg_op = (modrm >> 3) & 0x07;

        // Memory fences and FSGSBASE (mod=3, specific reg values)
        if modrm >> 6 == 3 {
            let rm = (modrm & 0x07) | ctx.rex_b(); // Apply REX.B for extended registers
            match reg_op {
                // WAITPKG register forms using the 0F AE /6 slot.
                6 if ctx.rep_prefix == Some(0xF3) => {
                    // UMONITOR r16/r32/r64 - arm monitor hardware. No visible state in emulation.
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                6 if ctx.rep_prefix == Some(0xF2) || ctx.operand_size_override => {
                    // UMWAIT/TPAUSE return immediately in emulation. For the deterministic
                    // zero-deadline case this matches hardware: CF=0 and other status flags clear.
                    self.clear_lazy_flags();
                    self.regs.rflags &= !(flags::bits::CF
                        | flags::bits::PF
                        | flags::bits::AF
                        | flags::bits::ZF
                        | flags::bits::SF
                        | flags::bits::OF);
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                // FSGSBASE instructions (require F3 prefix)
                0 if ctx.rep_prefix == Some(0xF3) => {
                    // RDFSBASE - Read FS base to register
                    let value = if ctx.rex_w() {
                        self.sregs.fs.base
                    } else {
                        self.sregs.fs.base & 0xFFFF_FFFF
                    };
                    self.set_reg(rm, value, if ctx.rex_w() { 8 } else { 4 });
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                1 if ctx.rep_prefix == Some(0xF3) => {
                    // RDGSBASE - Read GS base to register
                    let value = if ctx.rex_w() {
                        self.sregs.gs.base
                    } else {
                        self.sregs.gs.base & 0xFFFF_FFFF
                    };
                    self.set_reg(rm, value, if ctx.rex_w() { 8 } else { 4 });
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                2 if ctx.rep_prefix == Some(0xF3) => {
                    // WRFSBASE - Write register to FS base
                    let value = if ctx.rex_w() {
                        self.get_reg(rm, 8)
                    } else {
                        self.get_reg(rm, 4)
                    };
                    self.sregs.fs.base = value;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                3 if ctx.rep_prefix == Some(0xF3) => {
                    // WRGSBASE - Write register to GS base
                    let value = if ctx.rex_w() {
                        self.get_reg(rm, 8)
                    } else {
                        self.get_reg(rm, 4)
                    };
                    self.sregs.gs.base = value;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                5 if ctx.rep_prefix == Some(0xF3) => {
                    // INCSSPD/INCSSPQ are CET shadow-stack instructions. The
                    // emulator does not expose CET shadow stacks, so these
                    // F3-prefixed register forms are #UD instead of LFENCE.
                    self.inject_undefined_instruction()
                }
                5 => insn::system::lfence(self, ctx), // LFENCE (E8-EF)
                6 => insn::system::mfence(self, ctx), // MFENCE (F0-F7)
                7 => insn::system::sfence(self, ctx), // SFENCE (F8-FF)
                _ => {
                    self.inject_exception(6, None)?;
                    Ok(None)
                }
            }
        } else {
            if reg_op == 6 && ctx.rep_prefix == Some(0xF3) {
                // CLRSSBSY is a CET shadow-stack instruction. The emulator does
                // not expose CET shadow stacks, so this F3-prefixed memory form
                // is #UD instead of CLWB.
                self.inject_undefined_instruction()?;
                return Ok(None);
            }

            // Memory operand forms (FXSAVE, FXRSTOR, LDMXCSR, STMXCSR, XSAVE, XRSTOR, CLFLUSH)
            let modrm_start = ctx.cursor - 1;
            let (addr, extra) = self.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;

            match reg_op {
                0 => {
                    // FXSAVE - save FPU/SSE state (512 bytes)
                    // Zero the area first
                    for i in 0..64 {
                        self.write_mem(addr + i * 8, 0u64, 8)?;
                    }
                    // FCW at offset 0
                    self.write_mem16(addr, self.fpu.control_word)?;
                    // FSW at offset 2
                    self.write_mem16(addr + 2, self.fpu.status_word)?;
                    // Abridged FTW at offset 4 (1 byte, 1 bit per register)
                    let mut abtw = 0u8;
                    for i in 0..8 {
                        let tag = (self.fpu.tag_word >> (i * 2)) & 3;
                        if tag != 3 {
                            abtw |= 1 << i;
                        }
                    }
                    self.mmu.write_u8(addr + 4, abtw, &self.sregs)?;
                    // FOP at offset 6
                    self.write_mem16(addr + 6, self.fpu.last_opcode)?;
                    // FIP at offset 8 (8 bytes in 64-bit mode)
                    self.write_mem64(addr + 8, self.fpu.instr_ptr)?;
                    // FDP at offset 16 (8 bytes in 64-bit mode)
                    self.write_mem64(addr + 16, self.fpu.data_ptr)?;
                    // MXCSR at offset 24
                    self.write_mem32(addr + 24, self.mxcsr)?;
                    // MXCSR_MASK at offset 28
                    self.write_mem32(addr + 28, 0xFFFF)?;
                    // ST0-ST7 at offset 32 (16 bytes each)
                    for i in 0..8 {
                        let bytes = insn::fpu::f64_to_f80_pub(self.fpu.get_st(i as u8));
                        self.write_bytes(addr + 32 + (i as u64) * 16, &bytes)?;
                    }
                    // XMM0-XMM15 at offset 160 (16 bytes each)
                    for i in 0..16 {
                        let xmm = self.regs.xmm[i];
                        self.write_mem64(addr + 160 + (i as u64) * 16, xmm[0])?;
                        self.write_mem64(addr + 160 + (i as u64) * 16 + 8, xmm[1])?;
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                1 => {
                    // FXRSTOR - restore FPU/SSE state (512 bytes)
                    // FCW at offset 0
                    self.fpu.control_word = self.read_mem16(addr)?;
                    // FSW at offset 2
                    self.fpu.status_word = self.read_mem16(addr + 2)?;
                    self.fpu.top = ((self.fpu.status_word >> 11) & 7) as u8;
                    // Abridged FTW at offset 4
                    let abtw = self.mmu.read_u8(addr + 4, &self.sregs)?;
                    self.fpu.tag_word = 0;
                    for i in 0..8 {
                        if abtw & (1 << i) != 0 {
                            self.fpu.tag_word |= 0 << (i * 2); // Valid
                        } else {
                            self.fpu.tag_word |= 3 << (i * 2); // Empty
                        }
                    }
                    // FOP at offset 6
                    self.fpu.last_opcode = self.read_mem16(addr + 6)?;
                    // FIP at offset 8
                    self.fpu.instr_ptr = self.read_mem64(addr + 8)?;
                    // FDP at offset 16
                    self.fpu.data_ptr = self.read_mem64(addr + 16)?;
                    // MXCSR at offset 24
                    self.mxcsr = self.read_mem32(addr + 24)?;
                    // ST0-ST7 at offset 32
                    for i in 0..8 {
                        let bytes = self.read_bytes(addr + 32 + (i as u64) * 16, 10)?;
                        self.fpu.set_st(i as u8, insn::fpu::f80_to_f64_pub(&bytes));
                    }
                    // XMM0-XMM15 at offset 160
                    for i in 0..16 {
                        self.regs.xmm[i][0] = self.read_mem64(addr + 160 + (i as u64) * 16)?;
                        self.regs.xmm[i][1] = self.read_mem64(addr + 160 + (i as u64) * 16 + 8)?;
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                2 => {
                    // LDMXCSR - load MXCSR register from memory
                    self.mxcsr = self.read_mem32(addr)?;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                3 => {
                    // STMXCSR - store MXCSR register to memory
                    self.write_mem(addr, self.mxcsr as u64, 4)?;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                4 | 6
                    if reg_op == 4 || (ctx.rep_prefix.is_none() && !ctx.operand_size_override) =>
                {
                    // XSAVE/XSAVEOPT - save x87/SSE/AVX/AVX-512 state selected by
                    // (EDX:EAX) & XCR0. XSAVEOPT may omit clean components, but this
                    // conservative save model is architecturally valid for observable state.
                    let rfbm = ((self.regs.rax & 0xFFFF_FFFF) | (self.regs.rdx << 32)) & self.xcr0;
                    let mut xstate_bv = 0u64;
                    // Component 0 (x87): legacy region header + ST0-7.
                    if rfbm & 0x1 != 0 {
                        self.write_mem16(addr, self.fpu.control_word)?;
                        self.write_mem16(addr + 2, self.fpu.status_word)?;
                        let mut abtw = 0u8;
                        for i in 0..8 {
                            if (self.fpu.tag_word >> (i * 2)) & 3 != 3 {
                                abtw |= 1 << i;
                            }
                        }
                        self.mmu.write_u8(addr + 4, abtw, &self.sregs)?;
                        self.write_mem16(addr + 6, self.fpu.last_opcode)?;
                        self.write_mem64(addr + 8, self.fpu.instr_ptr)?;
                        self.write_mem64(addr + 16, self.fpu.data_ptr)?;
                        for i in 0..8 {
                            let bytes = insn::fpu::f64_to_f80_pub(self.fpu.get_st(i as u8));
                            self.write_bytes(addr + 32 + (i as u64) * 16, &bytes)?;
                        }
                        xstate_bv |= 0x1;
                    }
                    // Component 1 (SSE): MXCSR + XMM0-15.
                    if rfbm & 0x2 != 0 {
                        self.write_mem32(addr + 24, self.mxcsr)?;
                        self.write_mem32(addr + 28, 0xFFFF)?;
                        for i in 0..16 {
                            self.write_mem64(addr + 160 + (i as u64) * 16, self.regs.xmm[i][0])?;
                            self.write_mem64(
                                addr + 160 + (i as u64) * 16 + 8,
                                self.regs.xmm[i][1],
                            )?;
                        }
                        xstate_bv |= 0x2;
                    }
                    // Component 2 (AVX): upper 128 bits of YMM0-15 at offset 576.
                    if rfbm & 0x4 != 0 {
                        for i in 0..16 {
                            self.write_mem64(
                                addr + 576 + (i as u64) * 16,
                                self.regs.ymm_high[i][0],
                            )?;
                            self.write_mem64(
                                addr + 576 + (i as u64) * 16 + 8,
                                self.regs.ymm_high[i][1],
                            )?;
                        }
                        xstate_bv |= 0x4;
                    }
                    // Component 5 (opmask): k0-k7 at offset 1088.
                    if rfbm & (1 << 5) != 0 {
                        for i in 0..8 {
                            self.write_mem64(addr + 1088 + (i as u64) * 8, self.regs.k[i])?;
                        }
                        xstate_bv |= 1 << 5;
                    }
                    // Component 6 (ZMM_Hi256): upper 256 bits of ZMM0-15.
                    if rfbm & (1 << 6) != 0 {
                        for i in 0..16 {
                            for lane in 0..4 {
                                self.write_mem64(
                                    addr + 1152 + (i as u64) * 32 + (lane as u64) * 8,
                                    self.regs.zmm_high[i][lane],
                                )?;
                            }
                        }
                        xstate_bv |= 1 << 6;
                    }
                    // Component 7 (Hi16_ZMM): full ZMM16-31.
                    if rfbm & (1 << 7) != 0 {
                        for i in 0..16 {
                            for lane in 0..8 {
                                self.write_mem64(
                                    addr + 1664 + (i as u64) * 64 + (lane as u64) * 8,
                                    self.regs.zmm_ext[i][lane],
                                )?;
                            }
                        }
                        xstate_bv |= 1 << 7;
                    }
                    // Component 19 (APX_F): R16-R31 at offset 960.
                    if rfbm & (1 << 19) != 0 {
                        for i in 0..16 {
                            self.write_mem64(
                                addr + 960 + (i as u64) * 8,
                                self.get_reg(16 + i as u8, 8),
                            )?;
                        }
                        xstate_bv |= 1 << 19;
                    }
                    // XSAVE header (standard, non-compacted): XSTATE_BV + XCOMP_BV.
                    self.write_mem64(addr + 512, xstate_bv)?;
                    self.write_mem64(addr + 520, 0)?;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                5 => {
                    // XRSTOR - restore x87/SSE/AVX/AVX-512 state selected by (EDX:EAX) & XCR0.
                    if self.read_mem64(addr + 520)? & (1u64 << 63) != 0 {
                        if self.restore_xsave_compacted_area(addr)? {
                            self.regs.rip += ctx.cursor as u64;
                        }
                        return Ok(None);
                    }

                    let rfbm = ((self.regs.rax & 0xFFFF_FFFF) | (self.regs.rdx << 32)) & self.xcr0;
                    let xstate_bv = self.read_mem64(addr + 512)?;
                    if rfbm & 0x1 != 0 {
                        if xstate_bv & 0x1 != 0 {
                            self.fpu.control_word = self.read_mem16(addr)?;
                            self.fpu.status_word = self.read_mem16(addr + 2)?;
                            self.fpu.top = ((self.fpu.status_word >> 11) & 7) as u8;
                            let abtw = self.mmu.read_u8(addr + 4, &self.sregs)?;
                            self.fpu.tag_word = 0;
                            for i in 0..8 {
                                if abtw & (1 << i) == 0 {
                                    self.fpu.tag_word |= 3 << (i * 2);
                                }
                            }
                            self.fpu.last_opcode = self.read_mem16(addr + 6)?;
                            self.fpu.instr_ptr = self.read_mem64(addr + 8)?;
                            self.fpu.data_ptr = self.read_mem64(addr + 16)?;
                            for i in 0..8 {
                                let bytes = self.read_bytes(addr + 32 + (i as u64) * 16, 10)?;
                                self.fpu.set_st(i as u8, insn::fpu::f80_to_f64_pub(&bytes));
                            }
                        } else {
                            self.fpu.init();
                        }
                    }
                    if rfbm & 0x2 != 0 {
                        if xstate_bv & 0x2 != 0 {
                            self.mxcsr = self.read_mem32(addr + 24)?;
                            for i in 0..16 {
                                self.regs.xmm[i][0] =
                                    self.read_mem64(addr + 160 + (i as u64) * 16)?;
                                self.regs.xmm[i][1] =
                                    self.read_mem64(addr + 160 + (i as u64) * 16 + 8)?;
                            }
                        } else {
                            self.mxcsr = 0x1F80;
                            for i in 0..16 {
                                self.regs.xmm[i] = [0, 0];
                            }
                        }
                    }
                    if rfbm & 0x4 != 0 {
                        if xstate_bv & 0x4 != 0 {
                            for i in 0..16 {
                                self.regs.ymm_high[i][0] =
                                    self.read_mem64(addr + 576 + (i as u64) * 16)?;
                                self.regs.ymm_high[i][1] =
                                    self.read_mem64(addr + 576 + (i as u64) * 16 + 8)?;
                            }
                        } else {
                            for i in 0..16 {
                                self.regs.ymm_high[i] = [0, 0];
                            }
                        }
                    }
                    if rfbm & (1 << 5) != 0 {
                        if xstate_bv & (1 << 5) != 0 {
                            for i in 0..8 {
                                self.regs.k[i] = self.read_mem64(addr + 1088 + (i as u64) * 8)?;
                            }
                        } else {
                            self.regs.k = [0; 8];
                        }
                    }
                    if rfbm & (1 << 6) != 0 {
                        if xstate_bv & (1 << 6) != 0 {
                            for i in 0..16 {
                                for lane in 0..4 {
                                    self.regs.zmm_high[i][lane] = self.read_mem64(
                                        addr + 1152 + (i as u64) * 32 + (lane as u64) * 8,
                                    )?;
                                }
                            }
                        } else {
                            self.regs.zmm_high = [[0; 4]; 16];
                        }
                    }
                    if rfbm & (1 << 7) != 0 {
                        if xstate_bv & (1 << 7) != 0 {
                            for i in 0..16 {
                                for lane in 0..8 {
                                    self.regs.zmm_ext[i][lane] = self.read_mem64(
                                        addr + 1664 + (i as u64) * 64 + (lane as u64) * 8,
                                    )?;
                                }
                            }
                        } else {
                            self.regs.zmm_ext = [[0; 8]; 16];
                        }
                    }
                    if rfbm & (1 << 19) != 0 {
                        if xstate_bv & (1 << 19) != 0 {
                            for i in 0..16 {
                                let value = self.read_mem64(addr + 960 + (i as u64) * 8)?;
                                self.set_reg(16 + i as u8, value, 8);
                            }
                        } else {
                            for i in 0..16 {
                                self.set_reg(16 + i as u8, 0, 8);
                            }
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                6 => {
                    // CLWB - treat as NOP
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                7 => {
                    // CLFLUSH/CLFLUSHOPT - treat as NOP
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                _ => {
                    return Err(Error::Emulator(format!(
                        "unimplemented 0F AE /{} at RIP={:#x}",
                        reg_op, self.regs.rip
                    )));
                }
            }
        }
    }
}
