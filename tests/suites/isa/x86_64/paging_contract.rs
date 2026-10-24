//! Intel SDM Vol. 3A, paging permissions, reserved bits, and accessed/dirty flags.

use rax::error::Error;
use rax::isa::x86_64::{AccessType, Mmu};
use rax::vm::vcpu::SystemRegisters;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const VA: u64 = 0x8000;
const NX: u64 = 1 << 63;
const A: u64 = 1 << 5;
const D: u64 = 1 << 6;
const WP: u64 = 1 << 16;
const NXE: u64 = 1 << 11;

struct Machine {
    memory: Arc<GuestMemoryMmap>,
    mmu: Mmu,
    sregs: SystemRegisters,
    entries: Vec<u64>,
    physical: u64,
}

fn machine(shift: u32, va: u64) -> Machine {
    let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x400000)]).unwrap());
    let mut entries = Vec::new();
    let leaf_level = match shift {
        12 => 3,
        21 => 2,
        30 => 1,
        _ => panic!("unsupported page size"),
    };
    let base = match shift {
        12 => 0x80000,
        21 => 0x200000,
        30 => 0,
        _ => unreachable!(),
    };
    for level in 0..=leaf_level {
        let index = (va >> (39 - level * 9)) & 511;
        let address = (level as u64 + 1) * 0x1000 + index * 8;
        let value = if level == leaf_level {
            base | 7 | if shift == 12 { 0 } else { 128 }
        } else {
            (level as u64 + 2) * 0x1000 | 7
        };
        memory
            .write_obj(value.to_le(), GuestAddress(address))
            .unwrap();
        entries.push(address);
    }
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = (1 << 31) | WP | 1;
    sregs.cr3 = 0x1000;
    sregs.cr4 = 1 << 5;
    sregs.efer = (1 << 8) | (1 << 10) | NXE;
    sregs.cs.l = true;
    Machine {
        mmu: Mmu::new(memory.clone()),
        memory,
        sregs,
        entries,
        physical: base | (va & ((1 << shift) - 1)),
    }
}

impl Machine {
    fn entry(&self, level: usize) -> u64 {
        u64::from_le(
            self.memory
                .read_obj(GuestAddress(self.entries[level]))
                .unwrap(),
        )
    }
    fn change_entry(&self, level: usize, clear: u64, set: u64) {
        self.memory
            .write_obj(
                (self.entry(level) & !clear | set).to_le(),
                GuestAddress(self.entries[level]),
            )
            .unwrap();
    }
    fn fault(&mut self, va: u64, access: AccessType, code: u64) {
        assert!(
            matches!(self.mmu.translate(va, access, &self.sregs),
            Err(Error::PageFault { vaddr, error_code }) if vaddr == va && error_code == code),
            "expected page fault {code:#x} at {va:#x}"
        );
    }
}

#[test]
fn nx_permissions_apply_at_every_level_and_on_read_warmed_tlb_hits() {
    for shift in [12, 21, 30] {
        for level in 0..machine(shift, VA).entries.len() {
            for warm in [false, true] {
                let mut m = machine(shift, VA);
                m.change_entry(level, 0, NX);
                if warm {
                    assert_eq!(
                        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
                        m.physical
                    );
                }
                m.fault(VA, AccessType::Execute, 0x11);
            }
        }
    }
}

#[test]
fn nx_is_reserved_without_nxe_at_every_level() {
    for shift in [12, 21, 30] {
        for level in 0..machine(shift, VA).entries.len() {
            let mut m = machine(shift, VA);
            m.change_entry(level, 0, NX);
            m.sregs.efer &= !NXE;
            m.fault(VA, AccessType::Read, 9);
        }
    }
}

#[test]
fn write_protect_and_cpl_control_readonly_pages_at_every_level() {
    for shift in [12, 21, 30] {
        for level in 0..machine(shift, VA).entries.len() {
            for cpl in 0..=3 {
                for wp in [false, true] {
                    let mut m = machine(shift, VA);
                    m.change_entry(level, 2, 0);
                    m.sregs.cs.selector = cpl;
                    if !wp {
                        m.sregs.cr0 &= !WP;
                    }
                    if cpl != 3 && !wp {
                        assert_eq!(
                            m.mmu.translate(VA, AccessType::Write, &m.sregs).unwrap(),
                            m.physical
                        );
                    } else {
                        m.fault(VA, AccessType::Write, 3 | if cpl == 3 { 4 } else { 0 });
                    }
                }
            }
        }
    }
}

#[test]
fn cached_supervisor_permissions_are_rechecked_after_a_cpl_change() {
    for shift in [12, 21, 30] {
        for level in 0..machine(shift, VA).entries.len() {
            let mut m = machine(shift, VA);
            m.change_entry(level, 4, 0);
            m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap();
            m.sregs.cs.selector = 3;
            m.fault(VA, AccessType::Read, 5);
        }
    }
}

#[test]
fn successful_reads_and_writes_update_accessed_and_leaf_dirty_bits() {
    for shift in [12, 21, 30] {
        let mut m = machine(shift, VA);
        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap();
        for level in 0..m.entries.len() {
            assert_ne!(m.entry(level) & A, 0, "read shift={shift} level={level}");
            assert_eq!(m.entry(level) & D, 0);
        }
        m.mmu.translate(VA, AccessType::Write, &m.sregs).unwrap();
        for level in 0..m.entries.len() {
            assert_ne!(m.entry(level) & A, 0);
            assert_eq!(m.entry(level) & D != 0, level + 1 == m.entries.len());
        }
    }
}

#[test]
fn pml4_ps_is_reserved_but_leaf_pat_is_legal() {
    let mut m = machine(12, VA);
    m.change_entry(0, 0, 128);
    m.fault(VA, AccessType::Read, 9);
    for shift in [12, 21, 30] {
        let mut m = machine(shift, VA);
        m.change_entry(
            m.entries.len() - 1,
            0,
            if shift == 12 { 128 } else { 1 << 12 },
        );
        assert_eq!(
            m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
            m.physical
        );
    }
}

#[test]
fn direct_map_virtual_addresses_never_bypass_guest_page_tables() {
    let va = 0xffff888000008000;
    for cpl in [0, 3] {
        let mut m = machine(12, va);
        m.sregs.cs.selector = cpl;
        m.change_entry(3, 1, 0);
        m.fault(va, AccessType::Read, if cpl == 3 { 4 } else { 0 });
        let mut m = machine(12, va);
        m.sregs.cs.selector = cpl;
        m.change_entry(3, 2, 0);
        m.fault(va, AccessType::Write, 3 | if cpl == 3 { 4 } else { 0 });
        let mut m = machine(12, va);
        m.change_entry(0, 0, 128);
        m.fault(va, AccessType::Read, 9);
    }
}

#[path = "../../../support/x86_64/common/mod.rs"]
mod common;

#[test]
fn cpu_fetch_reports_page_fault_intent_and_preserves_faulting_rip() {
    use rax::vm::vcpu::VCpu;
    for nxe in [false, true] {
        for code_flags in [0, 0x1003, 0x1003 | NX] {
            let present = code_flags & 1 != 0;
            let (mut cpu, memory) = common::setup_vm(&[0x90], None);
            for (address, value) in [
                (0x20000, 0x21003u64),
                (0x21000, 0x22003),
                (0x22000, 0x23003),
            ] {
                memory
                    .write_obj(value.to_le(), GuestAddress(address))
                    .unwrap();
            }
            for page in 0..64u64 {
                let value = if page == 1 {
                    code_flags
                } else {
                    page * 0x1000 | 3
                };
                memory
                    .write_obj(value.to_le(), GuestAddress(0x23000 + page * 8))
                    .unwrap();
            }
            let mut sregs = cpu.get_sregs().unwrap();
            sregs.cr0 |= 1 << 31;
            sregs.cr3 = 0x20000;
            if nxe {
                sregs.efer |= NXE;
            } else {
                sregs.efer &= !NXE;
            }
            cpu.set_sregs(&sregs).unwrap();
            let expected = if present {
                if nxe { 0x11 } else { 9 }
            } else if nxe {
                0x10
            } else {
                0
            };
            let result = cpu.step();
            if code_flags == 0x1003 {
                result.unwrap();
                assert_eq!(cpu.get_regs().unwrap().rip, common::CODE_ADDR + 1);
                continue;
            }
            assert!(
                matches!(result, Err(Error::PageFault { vaddr, error_code })
                if vaddr == common::CODE_ADDR && error_code == expected),
                "nxe={nxe} present={present}: {result:?}"
            );
            let regs = cpu.get_regs().unwrap();
            assert_eq!(regs.rip, common::CODE_ADDR);
            assert_eq!(regs.rsp, common::STACK_ADDR);
        }
    }
}

#[test]
fn lower_nonpresence_takes_priority_over_upper_permission_restrictions() {
    for (clear, set, access, code, cpl) in [
        (4, 0, AccessType::Read, 4, 3),
        (2, 0, AccessType::Write, 2, 0),
        (0, NX, AccessType::Execute, 0x10, 0),
    ] {
        let mut m = machine(12, VA);
        m.change_entry(0, clear, set);
        m.change_entry(3, 1, 0);
        m.sregs.cs.selector = cpl;
        m.fault(VA, access, code);
    }
}

#[test]
fn address_width_and_large_page_alignment_reserved_bits_fault() {
    for shift in [12, 21, 30] {
        for level in 0..machine(shift, VA).entries.len() {
            let mut m = machine(shift, VA);
            m.change_entry(level, 0, 1 << 48);
            m.fault(VA, AccessType::Read, 9);
        }
    }
    for shift in [21, 30] {
        for bit in 13..shift {
            let mut m = machine(shift, VA);
            m.change_entry(m.entries.len() - 1, 0, 1 << bit);
            m.fault(VA, AccessType::Read, 9);
        }
    }
}

#[path = "paging_protection.rs"]
mod protection;

#[path = "paging_protection_mmu.rs"]
mod protection_mmu;

#[path = "paging_legacy32.rs"]
mod paging_legacy32;
