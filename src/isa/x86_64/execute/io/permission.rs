//! Architectural I/O-privilege and TSS permission-map checks.

use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;

const CR0_PE: u64 = 1 << 0;
const TSS_IO_MAP_BASE_OFFSET: u64 = 0x66;
const TSS_IO_MAP_BASE_LAST_OFFSET: u32 = 0x67;

/// Mutable control state that can differ from the owning vCPU while a native
/// region is in flight. Descriptor state remains in the vCPU because every
/// admitted instruction that changes TR or CS is a terminal native frontier.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Clone, Copy)]
pub(in crate::isa::x86_64) struct IoPermissionState {
    pub(in crate::isa::x86_64) cr0: u64,
    pub(in crate::isa::x86_64) cr3: u64,
    pub(in crate::isa::x86_64) cr4: u64,
    pub(in crate::isa::x86_64) efer: u64,
    pub(in crate::isa::x86_64) cpl: u8,
    pub(in crate::isa::x86_64) rflags: u64,
}

impl X86_64Vcpu {
    /// Enforce Intel's IOPL/TSS I/O-permission-map decision for one port
    /// transfer. The bitmap has one bit per byte port; a 1-, 2-, or 4-byte
    /// transfer tests that many consecutive bits. Runtime is O(1), space O(1).
    pub(super) fn check_io_permission(&mut self, port: u16, size: u8) -> Result<()> {
        let cpl = (self.sregs.cs.selector & 3) as u8;
        self.check_io_permission_with_state(
            port,
            size,
            self.sregs.cr0,
            self.sregs.cr3,
            self.sregs.cr4,
            self.sregs.efer,
            cpl,
            self.regs.rflags,
            false,
        )
    }

    /// Native-helper preflight of the same architectural decision. A bitmap
    /// access is accepted only when both bytes translate to ordinary RAM;
    /// faults, MMIO, and malformed TSS state deoptimize for exact direct replay.
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    pub(in crate::isa::x86_64) fn jit_io_permission_allowed(
        &mut self,
        port: u16,
        size: u8,
        state: IoPermissionState,
    ) -> bool {
        self.check_io_permission_with_state(
            port,
            size,
            state.cr0,
            state.cr3,
            state.cr4,
            state.efer,
            state.cpl,
            state.rflags,
            true,
        )
        .is_ok()
    }

    #[allow(clippy::too_many_arguments)]
    fn check_io_permission_with_state(
        &mut self,
        port: u16,
        size: u8,
        cr0: u64,
        cr3: u64,
        cr4: u64,
        efer: u64,
        cpl: u8,
        rflags: u64,
        speculative_plain_ram_only: bool,
    ) -> Result<()> {
        if !matches!(size, 1 | 2 | 4) {
            return Err(Error::GeneralProtection { error_code: 0 });
        }

        let virtual_8086 = rflags & flags::bits::VM != 0;
        let iopl = ((rflags & flags::bits::IOPL_MASK) >> 12) as u8;
        if cr0 & CR0_PE == 0 || (!virtual_8086 && cpl <= iopl) {
            return Ok(());
        }

        // Only 32-bit and 64-bit available/busy TSS descriptors (types 9/B)
        // carry the I/O-map-base field at offset 0x66. A missing map denies all
        // ports when CPL > IOPL or VM=1.
        let tr = &self.sregs.tr;
        if tr.selector & !3 == 0
            || tr.unusable
            || !tr.present
            || tr.s
            || !matches!(tr.type_ & 0x0F, 0x9 | 0xB)
            || tr.limit < TSS_IO_MAP_BASE_LAST_OFFSET
        {
            return Err(Error::GeneralProtection { error_code: 0 });
        }
        let tss_base = tr.base;
        let tss_limit = u64::from(tr.limit);

        let mut access_sregs = self.sregs.clone();
        access_sregs.cr0 = cr0;
        access_sregs.cr3 = cr3;
        access_sregs.cr4 = cr4;
        access_sregs.efer = efer;

        let result: Result<()> = (|| {
            let map_base_address = tss_base
                .checked_add(TSS_IO_MAP_BASE_OFFSET)
                .ok_or(Error::GeneralProtection { error_code: 0 })?;
            let mut map_base_bytes = [0_u8; 2];
            if speculative_plain_ram_only {
                #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
                if !self.mmu.read_supervisor_range_is_plain_ram(
                    map_base_address,
                    map_base_bytes.len(),
                    &access_sregs,
                ) {
                    return Err(Error::GeneralProtection { error_code: 0 });
                }
                #[cfg(not(all(feature = "smir-jit", target_arch = "x86_64")))]
                return Err(Error::GeneralProtection { error_code: 0 });
            }
            self.mmu
                .read_supervisor(map_base_address, &mut map_base_bytes, &access_sregs)?;
            let map_base = u64::from(u16::from_le_bytes(map_base_bytes));
            if map_base >= tss_limit {
                return Err(Error::GeneralProtection { error_code: 0 });
            }

            // Intel specifies a two-byte bitmap fetch for every port access. The
            // second byte must remain inside the inclusive TSS segment limit; this
            // also enforces the required all-one terminator for a complete map.
            let bitmap_offset = map_base
                .checked_add(u64::from(port >> 3))
                .ok_or(Error::GeneralProtection { error_code: 0 })?;
            let bitmap_last = bitmap_offset
                .checked_add(1)
                .ok_or(Error::GeneralProtection { error_code: 0 })?;
            if bitmap_last > tss_limit {
                return Err(Error::GeneralProtection { error_code: 0 });
            }
            let bitmap_address = tss_base
                .checked_add(bitmap_offset)
                .ok_or(Error::GeneralProtection { error_code: 0 })?;
            let mut bitmap_bytes = [0_u8; 2];
            if speculative_plain_ram_only {
                #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
                if !self.mmu.read_supervisor_range_is_plain_ram(
                    bitmap_address,
                    bitmap_bytes.len(),
                    &access_sregs,
                ) {
                    return Err(Error::GeneralProtection { error_code: 0 });
                }
                #[cfg(not(all(feature = "smir-jit", target_arch = "x86_64")))]
                return Err(Error::GeneralProtection { error_code: 0 });
            }
            self.mmu
                .read_supervisor(bitmap_address, &mut bitmap_bytes, &access_sregs)?;

            let first_bit = u32::from(port & 7);
            let access_mask = (((1_u16 << size) - 1) << first_bit) as u16;
            if u16::from_le_bytes(bitmap_bytes) & access_mask != 0 {
                return Err(Error::GeneralProtection { error_code: 0 });
            }
            Ok(())
        })();

        // The current TLB cache records translations but not the CPL under
        // which they were validated. Do not let an implicit supervisor TSS
        // read authorize a later CPL3 access through that cache.
        self.mmu.flush_tlb();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::vcpu::{Segment, VCpu, VcpuExit};
    use std::sync::Arc;
    use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

    const TSS_BASE: u64 = 0x1_0000;
    const IO_MAP_BASE: u16 = 0x68;
    const IO_MAP_BYTES: u64 = 65_536 / 8;
    const TSS_LIMIT: u32 = IO_MAP_BASE as u32 + IO_MAP_BYTES as u32;
    const MEMORY_BYTES: usize = 0x3_0000;

    fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
        let memory = Arc::new(
            GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), MEMORY_BYTES)]).unwrap(),
        );
        memory.write_slice(code, GuestAddress(0)).unwrap();
        memory
    }

    fn configure_valid_tss(vcpu: &mut X86_64Vcpu, memory: &GuestMemoryMmap) {
        vcpu.sregs.tr = Segment {
            base: TSS_BASE,
            limit: TSS_LIMIT,
            selector: 0x28,
            type_: 0x9,
            present: true,
            s: false,
            ..Segment::default()
        };
        memory
            .write_slice(
                &IO_MAP_BASE.to_le_bytes(),
                GuestAddress(TSS_BASE + TSS_IO_MAP_BASE_OFFSET),
            )
            .unwrap();
        memory
            .write_slice(
                &[0xFF],
                GuestAddress(TSS_BASE + u64::from(IO_MAP_BASE) + IO_MAP_BYTES),
            )
            .unwrap();
    }

    fn protected_vcpu(code: &[u8]) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
        let memory = memory_with_code(code);
        let mut vcpu = X86_64Vcpu::new(0, memory.clone());
        vcpu.sregs.cr0 = CR0_PE;
        vcpu.sregs.efer = 1 << 10;
        vcpu.sregs.cs.l = true;
        vcpu.sregs.cs.selector = 3;
        vcpu.regs.rflags = 0x2;
        configure_valid_tss(&mut vcpu, &memory);
        (vcpu, memory)
    }

    fn bitmap_byte(port: u16) -> u64 {
        TSS_BASE + u64::from(IO_MAP_BASE) + u64::from(port >> 3)
    }

    fn set_port_bit(memory: &GuestMemoryMmap, port: u16, denied: bool) {
        let address = GuestAddress(bitmap_byte(port));
        let mut byte = [0_u8; 1];
        memory.read_slice(&mut byte, address).unwrap();
        let mask = 1_u8 << (port & 7);
        if denied {
            byte[0] |= mask;
        } else {
            byte[0] &= !mask;
        }
        memory.write_slice(&byte, address).unwrap();
    }

    fn assert_gp(result: Result<()>, name: &str) {
        assert!(
            matches!(result, Err(Error::GeneralProtection { error_code: 0 })),
            "{name}: {result:?}"
        );
    }

    #[test]
    fn io_bitmap_checks_every_byte_bit_in_unaligned_transfers() {
        let (mut vcpu, memory) = protected_vcpu(&[]);
        let cases: &[(u16, u8)] = &[
            (0x0000, 1),
            (0x0007, 2),
            (0x0007, 4),
            (0x1234, 4),
            (0xFFFF, 1),
        ];

        for &(port, size) in cases {
            vcpu.check_io_permission(port, size)
                .unwrap_or_else(|error| panic!("port={port:#06x} size={size}: {error:?}"));
            for byte_offset in 0..u16::from(size) {
                let denied_port = port.checked_add(byte_offset).unwrap();
                set_port_bit(&memory, denied_port, true);
                assert_gp(
                    vcpu.check_io_permission(port, size),
                    &format!("port={port:#06x} size={size} denied={denied_port:#06x}"),
                );
                set_port_bit(&memory, denied_port, false);
            }
        }

        assert_gp(
            vcpu.check_io_permission(0xFFFF, 2),
            "word beyond port space",
        );
        assert_gp(
            vcpu.check_io_permission(0xFFFF, 4),
            "doubleword beyond port space",
        );
        for size in [0, 3, 8] {
            assert_gp(
                vcpu.check_io_permission(0, size),
                &format!("invalid width {size}"),
            );
        }
    }

    #[test]
    fn real_mode_iopl_and_virtual_8086_select_the_bitmap_exactly() {
        let memory = memory_with_code(&[]);
        let mut vcpu = X86_64Vcpu::new(0, memory.clone());
        vcpu.sregs.cs.selector = 3;
        vcpu.regs.rflags = 0x2;

        vcpu.check_io_permission(0x80, 1)
            .expect("real mode bypasses a missing TSS");

        vcpu.sregs.cr0 = CR0_PE;
        vcpu.regs.rflags = 0x2 | flags::bits::IOPL_MASK;
        vcpu.check_io_permission(0x80, 1)
            .expect("CPL3 <= IOPL3 bypasses a missing TSS");

        vcpu.sregs.cs.selector = 0;
        vcpu.regs.rflags = 0x2;
        vcpu.check_io_permission(0x80, 1)
            .expect("CPL0 <= IOPL0 bypasses a missing TSS");

        vcpu.sregs.cs.selector = 3;
        configure_valid_tss(&mut vcpu, &memory);
        set_port_bit(&memory, 0x80, true);
        vcpu.regs.rflags = 0x2 | flags::bits::IOPL_MASK | flags::bits::VM;
        assert_gp(
            vcpu.check_io_permission(0x80, 1),
            "virtual-8086 mode ignores IOPL bypass",
        );
        set_port_bit(&memory, 0x80, false);
        vcpu.check_io_permission(0x80, 1)
            .expect("virtual-8086 mode accepts a clear bitmap bit");
    }

    #[test]
    fn malformed_or_absent_tss_permission_maps_deny_fail_closed() {
        for case in 0..7 {
            let (mut vcpu, memory) = protected_vcpu(&[]);
            match case {
                0 => vcpu.sregs.tr.selector = 3,
                1 => vcpu.sregs.tr.unusable = true,
                2 => vcpu.sregs.tr.present = false,
                3 => vcpu.sregs.tr.s = true,
                4 => vcpu.sregs.tr.type_ = 0x3,
                5 => vcpu.sregs.tr.limit = TSS_IO_MAP_BASE_LAST_OFFSET - 1,
                6 => memory
                    .write_slice(
                        &(TSS_LIMIT as u16).to_le_bytes(),
                        GuestAddress(TSS_BASE + TSS_IO_MAP_BASE_OFFSET),
                    )
                    .unwrap(),
                _ => unreachable!(),
            }
            assert_gp(
                vcpu.check_io_permission(0, 1),
                &format!("malformed TSS case {case}"),
            );
        }

        let (mut truncated, memory) = protected_vcpu(&[]);
        truncated.sregs.tr.limit = u32::from(IO_MAP_BASE) + 1;
        memory
            .write_slice(
                &IO_MAP_BASE.to_le_bytes(),
                GuestAddress(TSS_BASE + TSS_IO_MAP_BASE_OFFSET),
            )
            .unwrap();
        assert_gp(
            truncated.check_io_permission(8, 1),
            "two-byte bitmap fetch crosses the TSS limit",
        );

        let (mut overflow, _) = protected_vcpu(&[]);
        overflow.sregs.tr.base = u64::MAX - 0x60;
        assert_gp(
            overflow.check_io_permission(0, 1),
            "TSS base arithmetic overflow",
        );
    }

    fn paged_tss_vcpu(map_bitmap_page: bool) -> X86_64Vcpu {
        const PML4: u64 = 0x1000;
        const PDPT: u64 = 0x2000;
        const PD: u64 = 0x3000;
        const PT: u64 = 0x4000;
        const DATA0: u64 = 0x6000;
        const DATA1: u64 = 0x7000;
        const PAGE_FLAGS: u64 = 0x3;
        const PAGED_TSS_BASE: u64 = 0x0F98;

        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x1_0000)]).unwrap());
        for (address, entry) in [
            (PML4, PDPT | PAGE_FLAGS),
            (PDPT, PD | PAGE_FLAGS),
            (PD, PT | PAGE_FLAGS),
            (PT, DATA0 | PAGE_FLAGS),
        ] {
            memory
                .write_slice(&entry.to_le_bytes(), GuestAddress(address))
                .unwrap();
        }
        if map_bitmap_page {
            memory
                .write_slice(&(DATA1 | PAGE_FLAGS).to_le_bytes(), GuestAddress(PT + 8))
                .unwrap();
        }
        memory
            .write_slice(&IO_MAP_BASE.to_le_bytes(), GuestAddress(DATA0 + 0x0FFE))
            .unwrap();

        let mut vcpu = X86_64Vcpu::new(0, memory);
        vcpu.sregs.cr0 = 0x8000_0001;
        vcpu.sregs.cr3 = PML4;
        vcpu.sregs.cr4 = 1 << 5;
        vcpu.sregs.efer = 0x500;
        vcpu.sregs.cs.l = true;
        vcpu.sregs.cs.selector = 3;
        vcpu.sregs.tr = Segment {
            base: PAGED_TSS_BASE,
            limit: 0x80,
            selector: 0x28,
            type_: 0xB,
            present: true,
            ..Segment::default()
        };
        vcpu.regs.rflags = 0x2;
        vcpu
    }

    #[test]
    fn tss_bitmap_reads_are_supervisor_accesses_and_page_fault_precise() {
        let mut mapped = paged_tss_vcpu(true);
        mapped
            .check_io_permission(0, 1)
            .expect("CPL3 implicit TSS reads ignore user-page permission");
        let mapped_sregs = mapped.sregs.clone();
        let mut byte = [0_u8; 1];
        assert!(matches!(
            mapped.mmu.read(0x1000, &mut byte, &mapped_sregs),
            Err(Error::PageFault {
                vaddr: 0x1000,
                error_code: 0x5,
            })
        ));

        let mut missing = paged_tss_vcpu(false);
        assert!(matches!(
            missing.check_io_permission(0, 1),
            Err(Error::PageFault {
                vaddr: 0x1000,
                error_code: 0,
            })
        ));

        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        {
            let state = IoPermissionState {
                cr0: missing.sregs.cr0,
                cr3: missing.sregs.cr3,
                cr4: missing.sregs.cr4,
                efer: missing.sregs.efer,
                cpl: 3,
                rflags: missing.regs.rflags,
            };
            assert!(
                !missing.jit_io_permission_allowed(0, 1, state),
                "native preflight must deoptimize before a faulting bitmap read"
            );
        }
    }

    #[test]
    fn scalar_and_string_permission_faults_do_not_commit_architectural_state() {
        for code in [&[0xE4, 0x80][..], &[0xE6, 0x80][..]] {
            let (mut vcpu, memory) = protected_vcpu(code);
            set_port_bit(&memory, 0x80, true);
            vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
            let before = vcpu.regs.clone();
            assert!(matches!(
                vcpu.step(),
                Err(Error::GeneralProtection { error_code: 0 })
            ));
            assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: RIP");
            assert_eq!(vcpu.regs.rax, before.rax, "{code:02X?}: RAX");
            vcpu.complete_io_in(&[0x5A, 0xA5, 0xC3, 0x3C]);
            assert_eq!(
                vcpu.regs.rax, before.rax,
                "{code:02X?}: no input request may be staged"
            );
        }

        let (mut outs, memory) = protected_vcpu(&[0x6E]);
        outs.regs.rdx = 0x80;
        outs.regs.rsi = MEMORY_BYTES as u64 + 0x1000;
        set_port_bit(&memory, 0x80, true);
        let before = outs.regs.clone();
        assert!(matches!(
            outs.step(),
            Err(Error::GeneralProtection { error_code: 0 })
        ));
        assert_eq!(outs.regs.rip, before.rip);
        assert_eq!(outs.regs.rsi, before.rsi);
        assert_eq!(outs.regs.rcx, before.rcx);
    }

    #[test]
    fn zero_count_rep_string_io_bypasses_permission_and_memory_access() {
        for code in [&[0xF3, 0x6C][..], &[0xF2, 0x6E][..]] {
            let memory = memory_with_code(code);
            let mut vcpu = X86_64Vcpu::new(0, memory);
            vcpu.sregs.cr0 = CR0_PE;
            vcpu.sregs.efer = 1 << 10;
            vcpu.sregs.cs.l = true;
            vcpu.sregs.cs.selector = 3;
            vcpu.regs.rflags = 0x2;
            vcpu.regs.rcx = 0;
            vcpu.regs.rsi = u64::MAX;
            vcpu.regs.rdi = u64::MAX;
            vcpu.regs.rdx = 0xFFFF;

            assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
            assert_eq!(vcpu.regs.rip, code.len() as u64, "{code:02X?}");
            assert_eq!(vcpu.regs.rcx, 0, "{code:02X?}");
            assert_eq!(vcpu.regs.rsi, u64::MAX, "{code:02X?}");
            assert_eq!(vcpu.regs.rdi, u64::MAX, "{code:02X?}");
        }
    }

    #[test]
    fn allowed_direct_scalar_io_preserves_flags_and_uses_little_endian_data() {
        let (mut vcpu, _) = protected_vcpu(&[0x66, 0xEF]);
        vcpu.sregs.cs.selector = 0;
        vcpu.regs.rdx = 0x03F8;
        vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
        vcpu.regs.rflags = 0x2 | flags::bits::CF | flags::bits::DF | flags::bits::OF;
        let flags_before = vcpu.regs.rflags;

        assert!(matches!(
            vcpu.step().unwrap(),
            Some(VcpuExit::IoOut { port: 0x03F8, data }) if data == [0xEF, 0xCD]
        ));
        assert_eq!(vcpu.regs.rip, 2);
        assert_eq!(vcpu.regs.rflags, flags_before);
    }
}
