use rax::error::Error;
use rax::isa::x86_64::X86_64Vcpu;
use rax::vm::vcpu::{MemAccess, SystemRegisters, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};
const ENTRY: u64 = 0x8000;
const DATA: u64 = 0x9000;
const SMAP: u64 = 1 << 21;
const SMEP: u64 = 1 << 20;
const PKE: u64 = 1 << 22;
const AC: u64 = 1 << 18;
fn machine(code: &[u8], controls: u64, key: u64) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x40000)]).unwrap());
    for (addr, value) in [
        (0x1000, 0x2007_u64),
        (0x2000, 0x3007),
        (0x3000, 0x4007),
        (0x4000 + (ENTRY >> 12) * 8, ENTRY | 3),
        (0x4000 + (DATA >> 12) * 8, DATA | 7 | (key << 59)),
    ] {
        memory.write_obj(value.to_le(), GuestAddress(addr)).unwrap();
    }
    memory.write_slice(code, GuestAddress(ENTRY)).unwrap();
    memory
        .write_obj(0x123456789abcdef0_u64, GuestAddress(DATA))
        .unwrap();
    let mut cpu = X86_64Vcpu::new(0, memory.clone());
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = (1 << 31) | (1 << 16) | 1;
    sregs.cr3 = 0x1000;
    sregs.cr4 = (1 << 5) | controls;
    sregs.efer = (1 << 8) | (1 << 10) | (1 << 11);
    sregs.cs.l = true;
    sregs.cs.selector = 8;
    sregs.cs.type_ = 11;
    sregs.cs.s = true;
    sregs.cs.present = true;
    cpu.set_sregs(&sregs).unwrap();
    let mut regs = cpu.get_regs().unwrap();
    regs.rip = ENTRY;
    regs.rbx = DATA;
    regs.rflags = 2;
    cpu.set_regs(&regs).unwrap();
    (cpu, memory)
}
fn fault(cpu: &mut X86_64Vcpu, address: u64, code: u64) {
    let before = cpu.get_regs().unwrap().rax;
    let result = cpu.step();
    assert!(
        matches!(result,Err(Error::PageFault{vaddr,error_code}) if vaddr==address&&error_code==code),
        "{result:?}"
    );
    assert_eq!(cpu.get_regs().unwrap().rax, before);
    assert_eq!(cpu.get_regs().unwrap().rip, ENTRY);
}
#[test]
fn smap_restricts_supervisor_reads_and_writes_when_ac_clear() {
    for (code, error) in [(&[0x48, 0x8b, 0x03][..], 1), (&[0x48, 0x89, 0x03][..], 3)] {
        for cpl in 0..3 {
            let (mut cpu, _) = machine(code, SMAP, 0);
            let mut sregs = cpu.get_sregs().unwrap();
            sregs.cs.selector = 8 | cpl;
            cpu.set_sregs(&sregs).unwrap();
            fault(&mut cpu, DATA, error);
        }
    }
}
#[test]
fn smap_allows_explicit_accesses_with_ac_set() {
    let (mut cpu, _) = machine(&[0x48, 0x8b, 0x03], SMAP, 0);
    let mut regs = cpu.get_regs().unwrap();
    regs.rflags |= AC;
    cpu.set_regs(&regs).unwrap();
    assert!(cpu.step().unwrap().is_none());
    assert_eq!(cpu.get_regs().unwrap().rax, 0x123456789abcdef0);
}
#[test]
fn wrpkru_restricts_the_following_supervisor_data_read() {
    let (mut cpu, _) = machine(&[0x0f, 0x01, 0xef, 0x48, 0x8b, 0x03], PKE, 1);
    let mut regs = cpu.get_regs().unwrap();
    regs.rax = 4;
    regs.rcx = 0;
    regs.rdx = 0;
    cpu.set_regs(&regs).unwrap();
    assert!(cpu.step().unwrap().is_none());
    assert_eq!(cpu.get_emulator_state().unwrap().pkru, 4);
    let before = cpu.get_regs().unwrap().rax;
    let result = cpu.step();
    assert!(
        matches!(
            result,
            Err(Error::PageFault {
                vaddr: DATA,
                error_code: 0x21
            })
        ),
        "{result:?}"
    );
    assert_eq!(cpu.get_regs().unwrap().rax, before);
    assert_eq!(cpu.get_regs().unwrap().rip, ENTRY + 3);
}
#[test]
fn pkru_changes_apply_to_warm_data_translations() {
    let (mut cpu, _) = machine(&[0x0f, 0x01, 0xef, 0x48, 0x8b, 0x03], PKE, 1);
    assert_eq!(cpu.translate_addr(DATA, MemAccess::Read).unwrap(), DATA);
    let mut regs = cpu.get_regs().unwrap();
    regs.rax = 4;
    regs.rcx = 0;
    regs.rdx = 0;
    cpu.set_regs(&regs).unwrap();
    assert!(cpu.step().unwrap().is_none());
    let result = cpu.step();
    assert!(
        matches!(
            result,
            Err(Error::PageFault {
                vaddr: DATA,
                error_code: 0x21
            })
        ),
        "{result:?}"
    );
}
#[test]
fn disabled_pke_ignores_nonzero_pkru() {
    let (mut cpu, _) = machine(&[0x48, 0x8b, 0x03], 0, 1);
    let mut state = cpu.get_emulator_state().unwrap();
    state.pkru = 4;
    cpu.set_emulator_state(&state).unwrap();
    assert!(cpu.step().unwrap().is_none());
    assert_eq!(cpu.get_regs().unwrap().rax, 0x123456789abcdef0);
}
#[test]
fn smep_restricts_user_instruction_fetches_on_walks_and_tlb_hits() {
    for warm in [false, true] {
        let (mut cpu, memory) = machine(&[0x90], SMEP, 0);
        memory
            .write_obj(
                (ENTRY | 7).to_le(),
                GuestAddress(0x4000 + (ENTRY >> 12) * 8),
            )
            .unwrap();
        if warm {
            cpu.translate_addr(ENTRY, MemAccess::Read).unwrap();
        }
        fault(&mut cpu, ENTRY, 0x11);
    }
}

#[test]
fn smep_hardening_does_not_expand_the_advertised_guest_control_profile() {
    let (mut cpu, _) = machine(&[0x0f, 0x22, 0xe0], 0, 0);
    let mut regs = cpu.get_regs().unwrap();
    regs.rax = (1 << 5) | SMEP;
    cpu.set_regs(&regs).unwrap();
    let result = cpu.step();
    // MOV-to-CR injects #GP; this fixture has no IDT, so delivery triple-faults.
    assert!(
        matches!(result,Err(Error::Emulator(ref diagnosis)) if diagnosis.starts_with("triple fault while delivering vector 13;")),
        "{result:?}"
    );
    assert_eq!(cpu.get_sregs().unwrap().cr4, 1 << 5);
    assert_eq!(cpu.get_regs().unwrap().rip, ENTRY);
}
