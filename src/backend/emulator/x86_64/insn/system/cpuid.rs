//! CPUID instruction.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// CPUID (0x0F 0xA2)
pub fn cpuid(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let leaf = vcpu.regs.rax as u32;
    let subleaf = vcpu.regs.rcx as u32;

    let (eax, ebx, ecx, edx) = match leaf {
        0 => {
            // Return max leaf and vendor string "GenuineIntel"
            // EBX = "Genu", EDX = "ineI", ECX = "ntel"
            (0x07, 0x756e6547, 0x49656e69, 0x6c65746e)
        }
        1 => {
            // Processor signature and features
            // EAX: Stepping=1, Model=15, Family=6 => 0x6F1 (typical x86-64)
            let signature: u32 = 0x000006F1;
            // EDX features (required by Linux: 0x0700a169)
            // bit 0: FPU, bit 3: PSE, bit 5: MSR, bit 6: PAE, bit 8: CX8
            // bit 9: APIC, bit 13: PGE, bit 15: CMOV, bit 19: CLFLUSH
            // bit 23: MMX, bit 24: FXSR, bit 25: SSE, bit 26: SSE2
            let features_edx: u32 = (1 << 0)   // FPU
                                  | (1 << 3)   // PSE
                                  | (1 << 5)   // MSR
                                  | (1 << 6)   // PAE
                                  | (1 << 8)   // CX8 (CMPXCHG8B) - REQUIRED
                                  | (1 << 9)   // APIC
                                  | (1 << 13)  // PGE - REQUIRED
                                  | (1 << 15)  // CMOV
                                  | (1 << 19)  // CLFLUSH
                                  | (1 << 23)  // MMX
                                  | (1 << 24)  // FXSR - REQUIRED
                                  | (1 << 25)  // SSE - REQUIRED
                                  | (1 << 26); // SSE2 - REQUIRED
            // ECX: SSE3(0), SSSE3(9), SSE4.1(19), SSE4.2(20), POPCNT(23)
            let features_ecx: u32 = (1 << 0) | (1 << 9) | (1 << 19) | (1 << 20) | (1 << 23);
            (signature, 0x00000000, features_ecx, features_edx)
        }
        2 => {
            // Cache and TLB information
            // AL = iteration count (always 1 for modern CPUs)
            // Format: each byte is a descriptor. 0 = null descriptor
            // Return a simple valid response
            (0x01, 0, 0, 0) // AL=1 = single iteration required
        }
        7 => {
            // Extended feature flags (subleaf 0)
            if subleaf == 0 {
                let ebx = 1u32 << 20; // SMAP
                let edx = 1u32 << 14; // SERIALIZE
                (0, ebx, 0, edx)
            } else {
                (0, 0, 0, 0)
            }
        }
        0x80000000 => {
            // Extended CPUID Information - max extended leaf
            (0x80000008u32, 0, 0, 0)
        }
        0x80000001 => {
            // Extended features - CRITICAL for efficient identity mapping
            // EAX: Same signature as leaf 1 (extended signature)
            let signature: u32 = 0x000006F1;
            let features_edx = (1u32 << 29)  // LM (Long Mode)
                             | (1u32 << 26)  // PDPE1GB (1GB huge pages in PDPTE)
                             | (1u32 << 20); // NX (No Execute)
            (signature, 0, 0, features_edx)
        }
        // Brand string: "Rax Emulator" padded to 48 bytes (3 leaves x 16 bytes)
        0x80000002 => {
            // "Rax Emulato" (first 12 chars = 3x u32)
            (0x20786152, 0x6c756d45, 0x726f7461, 0x00000000) // "Rax Emulator\0\0\0\0"
        }
        0x80000003 => {
            (0, 0, 0, 0) // Second part (empty/null)
        }
        0x80000004 => {
            (0, 0, 0, 0) // Third part (empty/null)
        }
        0x80000008 => {
            // Address sizes: physical bits, linear bits, number of cores
            // Use 36 bits (64GB) for a reasonable physical address space
            let phys_bits: u32 = 36; // 64GB physical address space
            let linear_bits: u32 = 48;
            (phys_bits | (linear_bits << 8), 0, 0, 0)
        }
        _ => (0, 0, 0, 0),
    };

    vcpu.regs.rax = eax as u64;
    vcpu.regs.rbx = ebx as u64;
    vcpu.regs.rcx = ecx as u64;
    vcpu.regs.rdx = edx as u64;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
