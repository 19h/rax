//! Intel SDM086 Vol.3A5.3 non-PAE paging and current CPUID-reserved fields.
use rax::error::Error;
use rax::isa::x86_64::{AccessType, Mmu};
use rax::vm::vcpu::SystemRegisters;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};
const VA: u64 = 0x00403020;
const P: u32 = 1;
const W: u32 = 2;
const U: u32 = 4;
const A: u32 = 32;
const D: u32 = 64;
struct Machine {
    memory: Arc<GuestMemoryMmap>,
    mmu: Mmu,
    sregs: SystemRegisters,
    pde: u64,
    pte: u64,
    large: bool,
}
fn machine(large: bool) -> Machine {
    let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = (1 << 31) | 1 | (1 << 16);
    sregs.cr3 = 0x1000;
    sregs.cr4 = if large { 1 << 4 } else { 0 };
    sregs.efer = 0;
    sregs.cs.l = false;
    sregs.cs.db = true;
    let pde = 0x1004;
    let pte = 0x200c;
    memory
        .write_obj(
            (if large {
                P | W | U | 128
            } else {
                0x2000 | P | W | U
            })
            .to_le(),
            GuestAddress(pde),
        )
        .unwrap();
    memory
        .write_obj((0x7000u32 | P | W | U).to_le(), GuestAddress(pte))
        .unwrap();
    Machine {
        mmu: Mmu::new(memory.clone()),
        memory,
        sregs,
        pde,
        pte,
        large,
    }
}
impl Machine {
    fn entry(&self, address: u64) -> u32 {
        u32::from_le(self.memory.read_obj(GuestAddress(address)).unwrap())
    }
    fn change(&self, address: u64, clear: u32, set: u32) {
        self.memory
            .write_obj(
                (self.entry(address) & !clear | set).to_le(),
                GuestAddress(address),
            )
            .unwrap();
    }
    fn expected(&self) -> u64 {
        if self.large { VA & 0x3fffff } else { 0x7020 }
    }
    fn fault(&mut self, access: AccessType, code: u64) {
        let result = self.mmu.translate(VA, access, &self.sregs);
        assert!(
            matches!(result,Err(Error::PageFault{vaddr:VA,error_code})if error_code==code),
            "{result:?}"
        );
    }
}
#[test]
fn non_pae_four_kib_and_four_mib_translate_and_touch_only_32bit_entries() {
    for large in [false, true] {
        let mut m = machine(large);
        for address in [m.pde - 4, m.pde + 4, m.pte - 4, m.pte + 4] {
            m.memory
                .write_obj(0xdeadbeefu32.to_le(), GuestAddress(address))
                .unwrap();
        }
        assert_eq!(
            m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
            m.expected()
        );
        assert_ne!(m.entry(m.pde) & A, 0);
        let leaf = if large { m.pde } else { m.pte };
        assert_ne!(m.entry(leaf) & A, 0);
        assert_eq!(m.entry(leaf) & D, 0);
        assert_eq!(
            m.mmu.translate(VA, AccessType::Write, &m.sregs).unwrap(),
            m.expected()
        );
        assert_ne!(m.entry(leaf) & D, 0);
        for address in [m.pde - 4, m.pde + 4, m.pte - 4, m.pte + 4] {
            assert_eq!(m.entry(address), 0xdeadbeef);
        }
    }
}
#[test]
fn legacy_effective_permissions_and_warm_tlb_cover_all_levels_cpl_wp_and_access() {
    let mut count = 0;
    for large in [false, true] {
        let mut m = machine(large);
        let levels = if large {
            vec![m.pde]
        } else {
            vec![m.pde, m.pte]
        };
        for level in levels {
            for user in [false, true] {
                for writable in [false, true] {
                    for cpl in 0..4 {
                        for wp in [false, true] {
                            for warm in [false, true] {
                                for access in
                                    [AccessType::Read, AccessType::Write, AccessType::Execute]
                                {
                                    m.change(m.pde, U | W | A | D, U | W);
                                    m.change(m.pte, U | W | A | D, U | W);
                                    m.change(
                                        level,
                                        U | W,
                                        if user { U } else { 0 } | if writable { W } else { 0 },
                                    );
                                    m.mmu.flush_tlb();
                                    m.sregs.cs.selector = 0;
                                    m.sregs.cr0 |= 1 << 16;
                                    if warm {
                                        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap();
                                    }
                                    m.sregs.cs.selector = cpl;
                                    m.sregs.cr0 =
                                        m.sregs.cr0 & !(1 << 16) | if wp { 1 << 16 } else { 0 };
                                    let write = matches!(access, AccessType::Write);
                                    let denied = (cpl == 3 && !user)
                                        || (write && !writable && (cpl == 3 || wp));
                                    if denied {
                                        m.fault(
                                            access,
                                            1 | u64::from(write) * 2 | u64::from(cpl == 3) * 4,
                                        );
                                        assert_eq!(
                                            m.entry(if large { m.pde } else { m.pte }) & D,
                                            0
                                        );
                                    } else {
                                        assert_eq!(
                                            m.mmu.translate(VA, access, &m.sregs).unwrap(),
                                            m.expected()
                                        );
                                    }
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 576);
}
#[test]
fn absent_entries_fault_without_reserved_bit_precedence() {
    for large in [false, true] {
        for directory in [false, true] {
            let mut m = machine(large);
            let address = if directory { m.pde } else { m.pte };
            if large && !directory {
                continue;
            }
            m.change(address, P, 0xfffff000);
            m.sregs.cs.selector = 3;
            m.fault(AccessType::Write, 6);
            assert_eq!(m.entry(address) & A, 0);
        }
    }
}
#[test]
fn pse_controls_large_page_selection_and_pat_reservations() {
    let mut m = machine(false);
    m.change(m.pde, 0, 128);
    assert_eq!(
        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
        0x7020
    );
    m.sregs.cr4 = 1 << 4;
    assert!(matches!(
        m.mmu.translate(VA, AccessType::Read, &m.sregs),
        Err(Error::PageFault { error_code: 9, .. })
    ));
    for bit in 12..22 {
        let mut m = machine(true);
        m.change(m.pde, 0, 1 << bit);
        m.fault(AccessType::Read, 9);
        assert_eq!(m.entry(m.pde) & A, 0);
    }
    let mut m = machine(false);
    m.change(m.pte, 0, 128);
    assert_eq!(
        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
        0x7020
    );
    m.sregs.cr4 = 1 << 4;
    m.fault(AccessType::Read, 9);
}
#[test]
fn legacy_cr3_ignores_high_and_low_nonaddress_bits() {
    let mut m = machine(false);
    m.sregs.cr3 = 0xdeadbeef00001fff;
    assert_eq!(
        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
        0x7020
    );
}
#[test]
fn disabled_paging_interval_invalidates_old_mappings() {
    let mut m = machine(false);
    assert_eq!(
        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
        0x7020
    );
    m.sregs.cr0 &= !(1 << 31);
    assert_eq!(m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(), VA);
    m.change(m.pte, 0xfffff000, 0x8000);
    m.sregs.cr0 |= 1 << 31;
    assert_eq!(
        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
        0x8020
    );
}
#[test]
fn legacy_paging_ignores_long_mode_protection_keys_and_nxe() {
    let mut m = machine(false);
    m.sregs.cr4 |= 1 << 22;
    m.sregs.efer = 1 << 11;
    m.mmu.set_data_access_state(0, 0xffffffff);
    for access in [AccessType::Read, AccessType::Write, AccessType::Execute] {
        assert_eq!(m.mmu.translate(VA, access, &m.sregs).unwrap(), 0x7020);
    }
}
#[test]
fn legacy_linear_width_is_normalized_before_cold_and_warm_translation() {
    let mut m = machine(false);
    for address in [VA | (1 << 32), VA, VA | (0xdeadbeef << 32)] {
        assert_eq!(
            m.mmu
                .translate(address, AccessType::Read, &m.sregs)
                .unwrap(),
            0x7020
        );
    }
}
