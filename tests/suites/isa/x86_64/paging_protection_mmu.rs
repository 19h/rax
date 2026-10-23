//! Permission predicates are derived from Intel SDM 086 Vol.3A 5.6/5.7.
use super::{A, D, VA, WP, machine};
use rax::error::Error;
use rax::isa::x86_64::AccessType;
use vm_memory::{Bytes, GuestAddress};
const AC: u64 = 1 << 18;
const SMAP: u64 = 1 << 21;
const SMEP: u64 = 1 << 20;
const PKE: u64 = 1 << 22;

#[test]
fn dynamic_protection_matrix_covers_keys_privilege_controls_and_warm_translations() {
    for shift in [12, 21, 30] {
        for key in 0_u32..16 {
            let mut m = machine(shift, VA);
            m.change_entry(m.entries.len() - 1, 15 << 59, u64::from(key) << 59);
            for warm in [false, true] {
                for cpl in 0..4 {
                    for ac in [false, true] {
                        for smap in [false, true] {
                            for pke in [false, true] {
                                for wp in [false, true] {
                                    for rights in 0..4_u32 {
                                        for access in [
                                            AccessType::Read,
                                            AccessType::Write,
                                            AccessType::Execute,
                                        ] {
                                            if warm {
                                                m.sregs.cs.selector = 0;
                                                m.sregs.cr4 = 1 << 5;
                                                m.mmu.set_data_access_state(0, 0);
                                                m.mmu
                                                    .translate(VA, AccessType::Read, &m.sregs)
                                                    .unwrap();
                                            } else {
                                                m.mmu.flush_tlb();
                                            }
                                            m.sregs.cs.selector = cpl;
                                            m.sregs.cr0 =
                                                (m.sregs.cr0 & !WP) | if wp { WP } else { 0 };
                                            m.sregs.cr4 = (1 << 5)
                                                | if smap { SMAP } else { 0 }
                                                | if pke { PKE } else { 0 };
                                            m.mmu.set_data_access_state(
                                                if ac { AC } else { 0 },
                                                rights << (2 * key),
                                            );
                                            let data = !matches!(access, AccessType::Execute);
                                            let write = matches!(access, AccessType::Write);
                                            let key_denied = data
                                                && pke
                                                && (rights & 1 != 0
                                                    || (write
                                                        && rights & 2 != 0
                                                        && (cpl == 3 || wp)));
                                            let smap_denied = data && smap && cpl != 3 && !ac;
                                            let result = m.mmu.translate(VA, access, &m.sregs);
                                            if key_denied || smap_denied {
                                                let code = 1
                                                    | u64::from(write) * 2
                                                    | u64::from(cpl == 3) * 4
                                                    | u64::from(key_denied) * 32;
                                                assert!(
                                                    matches!(result,Err(Error::PageFault{vaddr:VA,error_code}) if error_code==code),
                                                    "shift={shift} key={key} cpl={cpl} ac={ac} smap={smap} pke={pke} wp={wp} rights={rights} access={access:?} warm={warm}: {result:?}"
                                                );
                                            } else {
                                                assert_eq!(result.unwrap(), m.physical);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn smep_depends_on_effective_page_type_and_current_privilege() {
    for shift in [12, 21, 30] {
        for cpl in 0..4 {
            for supervisor_page in [false, true] {
                let mut m = machine(shift, VA);
                if supervisor_page {
                    m.change_entry(0, 4, 0);
                }
                m.sregs.cr4 |= SMEP;
                m.sregs.cs.selector = cpl;
                if cpl == 3 && supervisor_page {
                    m.fault(VA, AccessType::Execute, 0x15);
                } else if cpl != 3 && !supervisor_page {
                    m.fault(VA, AccessType::Execute, 0x11);
                } else {
                    assert_eq!(
                        m.mmu.translate(VA, AccessType::Execute, &m.sregs).unwrap(),
                        m.physical
                    );
                }
            }
        }
    }
}

#[test]
fn upper_level_user_bits_determine_whether_pkru_applies() {
    for shift in [12, 21, 30] {
        for supervisor_level in 0..machine(shift, VA).entries.len() {
            let mut m = machine(shift, VA);
            m.change_entry(m.entries.len() - 1, 0, 1 << 59);
            m.change_entry(supervisor_level, 4, 0);
            m.sregs.cr4 |= PKE;
            m.mmu.set_data_access_state(0, 4);
            assert_eq!(
                m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
                m.physical
            );
        }
    }
}

#[test]
fn nonleaf_keys_are_ignored_and_permission_faults_do_not_set_dirty() {
    for shift in [12, 21, 30] {
        let mut m = machine(shift, VA);
        for level in 0..m.entries.len() - 1 {
            m.change_entry(level, 0, 15 << 59);
        }
        m.sregs.cr4 |= PKE;
        m.mmu.set_data_access_state(0, 3 << 30);
        assert_eq!(
            m.mmu.translate(VA, AccessType::Write, &m.sregs).unwrap(),
            m.physical
        );
        let leaf = m.entries.len() - 1;
        m.change_entry(leaf, D, 1 << 59);
        m.mmu.flush_tlb();
        m.mmu.set_data_access_state(0, 4);
        m.fault(VA, AccessType::Write, 0x23);
        assert_eq!(m.entry(leaf) & D, 0);
        assert_ne!(m.entry(leaf) & A, 0);
        m.change_entry(leaf, 1, 0);
        m.mmu.flush_tlb();
        m.fault(VA, AccessType::Write, 2);
    }
}

#[test]
fn implicit_system_accesses_cannot_use_ac_to_bypass_smap_and_restore_context_after_faults() {
    let mut m = machine(12, VA);
    m.sregs.cr4 |= SMAP;
    m.sregs.cs.selector = 3;
    m.mmu.set_data_access_state(AC, 0);
    let mut bytes = [0_u8; 8];
    assert!(matches!(
        m.mmu.read_supervisor(VA, &mut bytes, &m.sregs),
        Err(Error::PageFault {
            vaddr: VA,
            error_code: 1
        })
    ));
    assert!(matches!(
        m.mmu.write_supervisor(VA, &bytes, &m.sregs),
        Err(Error::PageFault {
            vaddr: VA,
            error_code: 3
        })
    ));
    m.sregs.cs.selector = 0;
    assert_eq!(
        m.mmu.translate(VA, AccessType::Read, &m.sregs).unwrap(),
        m.physical
    );
}

#[test]
fn late_page_key_fault_preserves_all_bytes_of_a_cross_page_write() {
    let mut m = machine(12, VA);
    let next_leaf = m.entries[m.entries.len() - 1] + 8;
    m.memory
        .write_obj(
            (0x81000_u64 | 7 | (1 << 59)).to_le(),
            GuestAddress(next_leaf),
        )
        .unwrap();
    m.memory
        .write_slice(&[0x5a; 8], GuestAddress(0x80ffc))
        .unwrap();
    m.sregs.cr4 |= PKE;
    m.mmu.set_data_access_state(0, 4);
    let result = m.mmu.write(VA + 0xffc, &[0xa5; 8], &m.sregs);
    assert!(
        matches!(
            result,
            Err(Error::PageFault {
                vaddr: 0x9000,
                error_code: 0x23
            })
        ),
        "{result:?}"
    );
    let mut bytes = [0; 8];
    m.memory
        .read_slice(&mut bytes, GuestAddress(0x80ffc))
        .unwrap();
    assert_eq!(bytes, [0x5a; 8]);
}

#[test]
fn readonly_and_key_denials_combine_without_losing_the_pk_error_bit() {
    for (cpl, wp, rights, code) in [
        (0, true, 2, Some(0x23)),
        (3, false, 2, Some(0x27)),
        (0, false, 2, None),
        (0, false, 1, Some(0x23)),
    ] {
        let mut m = machine(12, VA);
        m.change_entry(m.entries.len() - 1, 2, 1 << 59);
        m.sregs.cs.selector = cpl;
        if !wp {
            m.sregs.cr0 &= !WP;
        }
        m.sregs.cr4 |= PKE;
        m.mmu.set_data_access_state(0, rights << 2);
        if let Some(code) = code {
            m.fault(VA, AccessType::Write, code);
        } else {
            assert_eq!(
                m.mmu.translate(VA, AccessType::Write, &m.sregs).unwrap(),
                m.physical
            );
        }
    }
}
