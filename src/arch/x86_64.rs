use std::fs::File;

use linux_loader::cmdline::Cmdline;
use linux_loader::configurator::linux::LinuxBootConfigurator;
use linux_loader::configurator::BootConfigurator;
use linux_loader::configurator::BootParams;
use linux_loader::loader::bootparam::{boot_e820_entry, boot_params};
use linux_loader::loader::{load_cmdline, BzImage, KernelLoader, KernelLoaderResult};
use tracing::{debug, info};
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

use crate::arch::{Arch, BootInfo};
#[cfg(feature = "kvm")]
use crate::backend::kvm::KvmVm;
use crate::config::VmConfig;
use crate::cpu::{CpuState, DescriptorTable, Registers, Segment, SystemRegisters};
use crate::devices::bus::{IoBus, IoRange};
use crate::devices::pci::{PciStub, PCI_CONFIG_ADDRESS};
use crate::devices::rtc::{RtcStub, RTC_ADDRESS};
use crate::error::{Error, Result};
use crate::memory::{align_down, PAGE_SIZE};

const KERNEL_LOAD_ADDR: u64 = 0x100000;
const BOOT_PARAMS_ADDR: u64 = 0x7000;
const CMDLINE_ADDR: u64 = 0x20000;
const GDT_ADDR: u64 = 0x500;
const TSS_ADDR: u64 = 0x1000;
const BOOT_STACK_ADDR: u64 = 0x8ff0;

// Page table addresses for identity mapping
const PML4_ADDR: u64 = 0x9000;
const PDPTE_ADDR: u64 = 0xa000;
const PDE_ADDR: u64 = 0xb000;

const BOOT_CS: u16 = 0x10;
const BOOT_DS: u16 = 0x18;
const BOOT_TR: u16 = 0x20;

// x86 control register bits
const X86_CR0_PE: u64 = 1 << 0;
const X86_CR0_PG: u64 = 1 << 31;
const X86_CR4_PAE: u64 = 1 << 5;
const EFER_LME: u64 = 1 << 8;
const EFER_LMA: u64 = 1 << 10;

const E820_RAM: u32 = 1;
const E820_RESERVED: u32 = 2;

const LOW_MEM_END: u64 = 0x9fc00;
const BIOS_END: u64 = 0x100000;

const KVM_RESERVED_SIZE: u64 = 0x4000;

pub struct X86_64Arch;

impl X86_64Arch {
    pub fn new() -> Self {
        X86_64Arch
    }

    fn build_cmdline(cmdline: &str) -> Result<Cmdline> {
        let mut cmdline_builder =
            Cmdline::new(4096).map_err(|e| Error::InvalidConfig(format!("invalid cmdline: {e}")))?;
        cmdline_builder
            .insert_str(cmdline)
            .map_err(|e| Error::InvalidConfig(format!("invalid cmdline: {e}")))?;
        Ok(cmdline_builder)
    }

    fn load_kernel_image(mem: &GuestMemoryMmap, kernel: &VmConfig) -> Result<KernelLoaderResult> {
        info!(path = %kernel.kernel.display(), "loading kernel image");
        let mut kernel_file = File::open(&kernel.kernel)?;
        BzImage::load(
            mem,
            Some(GuestAddress(KERNEL_LOAD_ADDR)),
            &mut kernel_file,
            Some(GuestAddress(KERNEL_LOAD_ADDR)),
        )
        .map_err(Error::from)
    }

    fn load_initrd(
        mem: &GuestMemoryMmap,
        initrd_path: &std::path::Path,
        max_addr: u64,
        kernel_end: u64,
    ) -> Result<(GuestAddress, u64)> {
        info!(path = %initrd_path.display(), "loading initrd");
        let buf = std::fs::read(initrd_path)?;
        let size = buf.len() as u64;
        if size == 0 {
            return Err(Error::KernelLoad("initrd is empty".to_string()));
        }
        if max_addr < size {
            return Err(Error::KernelLoad(
                "initrd does not fit in guest memory".to_string(),
            ));
        }
        let start = align_down(max_addr.saturating_sub(size), PAGE_SIZE);
        if start < kernel_end {
            return Err(Error::KernelLoad(
                "initrd overlaps kernel image".to_string(),
            ));
        }
        let start_addr = GuestAddress(start);
        mem.write_slice(&buf, start_addr)?;
        Ok((start_addr, size))
    }

    fn build_e820(mem_size: u64, reserved_start: u64) -> Vec<boot_e820_entry> {
        let mut entries = Vec::new();
        if LOW_MEM_END > 0 {
            entries.push(boot_e820_entry {
                addr: 0,
                size: LOW_MEM_END,
                type_: E820_RAM,
            });
        }
        if BIOS_END > LOW_MEM_END {
            entries.push(boot_e820_entry {
                addr: LOW_MEM_END,
                size: BIOS_END - LOW_MEM_END,
                type_: E820_RESERVED,
            });
        }
        if reserved_start > BIOS_END {
            entries.push(boot_e820_entry {
                addr: BIOS_END,
                size: reserved_start - BIOS_END,
                type_: E820_RAM,
            });
        }
        if mem_size > reserved_start {
            entries.push(boot_e820_entry {
                addr: reserved_start,
                size: mem_size - reserved_start,
                type_: E820_RESERVED,
            });
        }
        entries
    }

    /// Build initial system registers for 64-bit long mode boot.
    fn build_sregs(_boot: &BootInfo) -> SystemRegisters {
        // 64-bit code segment (L=1, D=0 for long mode)
        let cs = Segment {
            base: 0,
            limit: 0xfffff,
            selector: BOOT_CS,
            type_: 0x0b, // Execute/Read, accessed
            present: true,
            dpl: 0,
            db: false, // Must be 0 for 64-bit code
            s: true,
            l: true, // 64-bit mode
            g: true,
            avl: false,
            unusable: false,
        };

        // Data segment
        let ds = Segment {
            base: 0,
            limit: 0xfffff,
            selector: BOOT_DS,
            type_: 0x03, // Read/Write, accessed
            present: true,
            dpl: 0,
            db: true,
            s: true,
            l: false,
            g: true,
            avl: false,
            unusable: false,
        };

        // TSS segment
        let tr = Segment {
            base: TSS_ADDR,
            limit: 0x67, // Minimum TSS size - 1
            selector: BOOT_TR,
            type_: 0x0b, // 32-bit busy TSS
            present: true,
            dpl: 0,
            db: false,
            s: false, // System segment
            l: false,
            g: false,
            avl: false,
            unusable: false,
        };

        // LDT (not used but needs valid state)
        let ldt = Segment {
            base: 0,
            limit: 0xffff,
            selector: 0,
            type_: 2,
            present: true,
            dpl: 0,
            db: false,
            s: false,
            l: false,
            g: false,
            avl: false,
            unusable: true,
        };

        SystemRegisters {
            cs,
            ds: ds.clone(),
            es: ds.clone(),
            fs: ds.clone(),
            gs: ds.clone(),
            ss: ds,
            tr,
            ldt,
            gdt: DescriptorTable {
                base: GDT_ADDR,
                limit: (5 * 8 - 1) as u16,
            },
            idt: DescriptorTable {
                base: 0,
                limit: 0xffff,
            },
            cr0: X86_CR0_PE | X86_CR0_PG | 0x20, // PE + PG + NE
            cr2: 0,
            cr3: PML4_ADDR,
            cr4: X86_CR4_PAE,
            cr8: 0,
            efer: EFER_LME | EFER_LMA,
        }
    }

    /// Set up identity-mapped page tables for the first 1GB.
    fn setup_page_tables(mem: &GuestMemoryMmap) -> Result<()> {
        // PML4 entry pointing to PDPTE
        let pml4_entry: u64 = PDPTE_ADDR | 0x3; // Present + Writable
        mem.write_obj(pml4_entry, GuestAddress(PML4_ADDR))?;

        // PDPTE entry pointing to PDE
        let pdpte_entry: u64 = PDE_ADDR | 0x3; // Present + Writable
        mem.write_obj(pdpte_entry, GuestAddress(PDPTE_ADDR))?;

        // 512 PDE entries, each mapping 2MB (using huge pages)
        for i in 0u64..512 {
            let pde_entry: u64 = (i << 21) | 0x83; // Present + Writable + Huge (2MB page)
            let addr = GuestAddress(PDE_ADDR + i * 8);
            mem.write_obj(pde_entry, addr)?;
        }

        debug!(
            pml4 = format!("{:#x}", PML4_ADDR),
            pdpte = format!("{:#x}", PDPTE_ADDR),
            pde = format!("{:#x}", PDE_ADDR),
            "setup page tables for 1GB identity mapping"
        );
        Ok(())
    }

    fn write_gdt(mem: &GuestMemoryMmap) -> Result<()> {
        // Build TSS descriptor (16 bytes for 32-bit TSS)
        let tss_base = TSS_ADDR;
        let tss_limit: u32 = 0x67;

        // 32-bit TSS descriptor (type 0x89 = available 32-bit TSS)
        let tss_low = (tss_limit & 0xffff) as u64
            | ((tss_base & 0xffffff) << 16)
            | (0x89u64 << 40)
            | ((tss_limit as u64 >> 16) & 0xf) << 48
            | ((tss_base >> 24) & 0xff) << 56;

        // 64-bit code segment: L=1, D=0
        let code64_entry = gdt_entry_64bit(0x9a);

        let gdt = [
            0u64,         // 0x00: null
            0u64,         // 0x08: null
            code64_entry, // 0x10: 64-bit code segment
            gdt_entry(0x92), // 0x18: data segment
            tss_low,      // 0x20: TSS descriptor
        ];
        debug!(
            gdt_entries = format!("{:#018x?}", gdt),
            gdt_addr = format!("{:#x}", GDT_ADDR),
            "writing GDT"
        );
        for (index, entry) in gdt.iter().enumerate() {
            let addr = GuestAddress(GDT_ADDR + (index as u64 * 8));
            mem.write_obj(*entry, addr)?;
        }

        // Write a minimal TSS at TSS_ADDR
        let tss = [0u8; 104];
        mem.write_slice(&tss, GuestAddress(TSS_ADDR))?;

        debug!(tss_addr = format!("{:#x}", TSS_ADDR), "wrote TSS");
        Ok(())
    }
}

impl Arch for X86_64Arch {
    fn name(&self) -> &'static str {
        "x86_64"
    }

    fn setup_devices(&self, io_bus: &mut IoBus) -> Result<()> {
        // PCI configuration space (stub - returns no devices)
        io_bus.register(
            IoRange {
                base: PCI_CONFIG_ADDRESS,
                len: 8,
            },
            Box::new(PciStub::new()),
        )?;

        // CMOS/RTC
        io_bus.register(
            IoRange {
                base: RTC_ADDRESS,
                len: 2,
            },
            Box::new(RtcStub::new()),
        )?;

        Ok(())
    }

    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo> {
        let mem_size = mem.last_addr().raw_value() + 1;
        if mem_size <= KERNEL_LOAD_ADDR + KVM_RESERVED_SIZE {
            return Err(Error::InvalidConfig(
                "memory is too small for kernel and reserved pages".to_string(),
            ));
        }

        let reserved_start = align_down(mem_size - KVM_RESERVED_SIZE, PAGE_SIZE);
        let identity_map_addr = reserved_start;
        let tss_addr = reserved_start + PAGE_SIZE;

        let loader_result = Self::load_kernel_image(mem, config)?;
        let setup_header = loader_result
            .setup_header
            .ok_or_else(|| Error::KernelLoad("missing setup header".to_string()))?;

        let hdr_version = { setup_header.version };
        let hdr_loadflags = { setup_header.loadflags };
        let hdr_code32_start = { setup_header.code32_start };
        debug!(
            version = format!("{:#x}", hdr_version),
            loadflags = format!("{:#x}", hdr_loadflags),
            code32_start = format!("{:#x}", hdr_code32_start),
            "setup header"
        );

        let cmdline = Self::build_cmdline(&config.cmdline)?;
        load_cmdline(mem, GuestAddress(CMDLINE_ADDR), &cmdline).map_err(Error::from)?;
        let cmdline_size = cmdline
            .as_cstring()
            .map_err(|e| Error::KernelLoad(format!("cmdline error: {e}")))?
            .as_bytes_with_nul()
            .len() as u32;
        if CMDLINE_ADDR + cmdline_size as u64 >= BIOS_END {
            return Err(Error::KernelLoad("cmdline exceeds low memory".to_string()));
        }

        let mut params = boot_params::default();
        params.hdr = setup_header;
        params.hdr.type_of_loader = 0xff;
        params.hdr.loadflags |= 0x1 | 0x40; // LOADED_HIGH + KEEP_SEGMENTS
        params.hdr.cmd_line_ptr = CMDLINE_ADDR as u32;
        params.hdr.cmdline_size = cmdline_size;

        let kernel_end = loader_result.kernel_end as u64;
        if kernel_end >= reserved_start {
            return Err(Error::KernelLoad(
                "kernel image overlaps reserved KVM region".to_string(),
            ));
        }
        if let Some(initrd_path) = &config.initrd {
            let initrd_addr_max = if params.hdr.initrd_addr_max == 0 {
                reserved_start - 1
            } else {
                params.hdr.initrd_addr_max as u64
            };
            let initrd_max = initrd_addr_max.min(reserved_start - 1);
            let (initrd_addr, initrd_size) =
                Self::load_initrd(mem, initrd_path, initrd_max, kernel_end)?;
            params.hdr.ramdisk_image = initrd_addr.raw_value() as u32;
            params.hdr.ramdisk_size = initrd_size as u32;
        }

        let e820_entries = Self::build_e820(mem_size, reserved_start);
        debug!(entries = e820_entries.len(), "built e820 map");
        params.e820_entries = e820_entries.len() as u8;
        for (index, entry) in e820_entries.iter().enumerate() {
            params.e820_table[index] = *entry;
        }

        let boot_params = BootParams::new(&params, GuestAddress(BOOT_PARAMS_ADDR));
        LinuxBootConfigurator::write_bootparams(&boot_params, mem)?;

        // Verify kernel is loaded by reading first bytes
        let mut first_bytes = [0u8; 16];
        mem.read_slice(
            &mut first_bytes,
            GuestAddress(loader_result.kernel_load.raw_value()),
        )
        .map_err(|e| Error::KernelLoad(format!("failed to read kernel: {e}")))?;
        debug!(
            entry = format!("{:#x}", loader_result.kernel_load.raw_value()),
            kernel_end = format!("{:#x}", loader_result.kernel_end),
            first_bytes = format!("{:02x?}", first_bytes),
            "kernel loaded"
        );

        // For 64-bit boot, use startup_64 at offset 0x200 from the 32-bit entry
        let entry_point_64 = loader_result.kernel_load.raw_value() + 0x200;
        debug!(
            entry32 = format!("{:#x}", loader_result.kernel_load.raw_value()),
            entry64 = format!("{:#x}", entry_point_64),
            "kernel entry points"
        );

        Ok(BootInfo {
            entry_point: entry_point_64,
            boot_params_addr: GuestAddress(BOOT_PARAMS_ADDR),
            tss_addr,
            identity_map_addr,
        })
    }

    #[cfg(feature = "kvm")]
    fn init_vm(&self, vm: &KvmVm, boot: &BootInfo) -> Result<()> {
        vm.create_irq_chip()?;
        vm.create_pit2()?;
        vm.set_tss_address(boot.tss_addr)?;
        vm.set_identity_map_address(boot.identity_map_addr)?;
        Ok(())
    }

    fn initial_cpu_state(&self, mem: &GuestMemoryMmap, boot: &BootInfo) -> Result<CpuState> {
        // Setup page tables and GDT in guest memory
        Self::setup_page_tables(mem)?;
        Self::write_gdt(mem)?;

        // Build initial CPU state
        let regs = Registers {
            rip: boot.entry_point,
            rflags: 0x2,
            rsp: BOOT_STACK_ADDR,
            rbp: 0,
            rbx: 0,
            rdi: 0,
            rsi: boot.boot_params_addr.raw_value(),
            ..Default::default()
        };

        let sregs = Self::build_sregs(boot);

        Ok(CpuState { regs, sregs })
    }
}

fn gdt_entry(access: u8) -> u64 {
    let flags: u64 = 0xcf; // G=1, D/B=1, L=0, AVL=1
    let limit: u64 = 0xffff;
    let base: u64 = 0;
    (limit & 0xffff)
        | ((base & 0xffffff) << 16)
        | ((access as u64) << 40)
        | ((flags as u64) << 48)
        | ((base >> 24) << 56)
}

/// Create a 64-bit code segment GDT entry (L=1, D=0)
fn gdt_entry_64bit(access: u8) -> u64 {
    let flags: u64 = 0xaf; // G=1, D/B=0, L=1, AVL=1 (64-bit mode)
    let limit: u64 = 0xffff;
    let base: u64 = 0;
    (limit & 0xffff)
        | ((base & 0xffffff) << 16)
        | ((access as u64) << 40)
        | ((flags as u64) << 48)
        | ((base >> 24) << 56)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_e820_layout() {
        let entries = X86_64Arch::build_e820(512 * 1024 * 1024, 0x1fffc000);
        assert!(entries.len() >= 3);
        let first_type =
            unsafe { std::ptr::read_unaligned(std::ptr::addr_of!(entries[0].type_)) };
        assert_eq!(first_type, E820_RAM);
    }
}
