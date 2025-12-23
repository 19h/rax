//! Two-byte opcode instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::super::aes;
use super::super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::super::flags;
use super::super::super::super::insn;

impl X86_64Vcpu {
    pub(in crate::backend::emulator::x86_64) fn execute_0f01(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        // Peek at modrm to determine instruction
        let modrm = ctx.peek_u8()?;

        // Check for special instructions with mod=3
        if modrm >> 6 == 3 {
            match modrm {
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
                    // XGETBV (0x0F 0x01 0xD0) - Get extended control register
                    ctx.consume_u8()?; // consume modrm
                                       // ECX specifies which XCR (only XCR0 is typically supported)
                                       // Returns XCR value in EDX:EAX (zero-extended in 64-bit mode)
                                       // For XCR0, return x87 bit set (bit 0) and SSE bit (bit 1)
                    let xcr0 = 0x03u64; // x87 + SSE always enabled
                                        // In 64-bit mode, writes to EAX/EDX zero-extend to RAX/RDX
                    self.regs.rax = xcr0 & 0xFFFFFFFF;
                    self.regs.rdx = (xcr0 >> 32) & 0xFFFFFFFF;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD1 => {
                    // XSETBV (0x0F 0x01 0xD1) - Set extended control register
                    ctx.consume_u8()?; // consume modrm
                                       // In emulator, just NOP (we ignore the write)
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD5 => {
                    // XEND (0x0F 0x01 0xD5) - End transaction
                    ctx.consume_u8()?; // consume modrm
                                       // TSX not supported, treat as NOP
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD6 => {
                    // XTEST (0x0F 0x01 0xD6) - Test if in transactional execution
                    ctx.consume_u8()?; // consume modrm
                                       // TSX not supported, ZF=1 (not in transaction)
                    self.regs.rflags |= flags::bits::ZF;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xF8 => {
                    // SWAPGS (0x0F 0x01 0xF8) - Swap GS base with IA32_KERNEL_GS_BASE
                    ctx.consume_u8()?; // consume modrm
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
    pub(in crate::backend::emulator::x86_64) fn execute_0fae(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.consume_u8()?;
        let reg_op = (modrm >> 3) & 0x07;

        // Memory fences (mod=3, specific reg values)
        if modrm >> 6 == 3 {
            match reg_op {
                5 => insn::system::lfence(self, ctx), // LFENCE (E8-EF)
                6 => insn::system::mfence(self, ctx), // MFENCE (F0-F7)
                7 => insn::system::sfence(self, ctx), // SFENCE (F8-FF)
                _ => {
                    return Err(Error::Emulator(format!(
                        "unimplemented 0F AE /{} (mod=3) at RIP={:#x}",
                        reg_op, self.regs.rip
                    )));
                }
            }
        } else {
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
                    self.write_mem32(addr + 24, 0x1F80)?;
                    // MXCSR_MASK at offset 28
                    self.write_mem32(addr + 28, 0xFFFF)?;
                    // ST0-ST7 at offset 32 (16 bytes each)
                    for i in 0..8 {
                        let bytes = insn::fpu::f64_to_f80_pub(self.fpu.st[i]);
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
                    // ST0-ST7 at offset 32
                    for i in 0..8 {
                        let bytes = self.read_bytes(addr + 32 + (i as u64) * 16, 10)?;
                        self.fpu.st[i] = insn::fpu::f80_to_f64_pub(&bytes);
                    }
                    // XMM0-XMM15 at offset 160
                    for i in 0..16 {
                        self.regs.xmm[i][0] = self.read_mem64(addr + 160 + (i as u64) * 16)?;
                        self.regs.xmm[i][1] =
                            self.read_mem64(addr + 160 + (i as u64) * 16 + 8)?;
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                2 => {
                    // LDMXCSR - load MXCSR register from memory
                    // Just skip - treat as NOP
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                3 => {
                    // STMXCSR - store MXCSR register to memory
                    // Store default MXCSR value (0x1F80)
                    self.write_mem(addr, 0x1F80u64, 4)?;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                4 => {
                    // XSAVE - save extended processor state
                    // EDX:EAX specifies which components to save
                    // Write minimal XSAVE area header (64 bytes)
                    // XSTATE_BV at offset 0 (8 bytes) - saved components
                    let xcr0 = 0x03u64; // x87 + SSE
                    self.write_mem(addr + 512, xcr0, 8)?; // XSTATE_BV in header
                    self.write_mem(addr + 520, 0u64, 8)?; // XCOMP_BV
                                                           // Zero rest of legacy region
                    for i in 0..64 {
                        self.write_mem(addr + i * 8, 0u64, 8)?;
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                5 => {
                    // XRSTOR - restore extended processor state
                    // Just skip - we don't actually restore state
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
