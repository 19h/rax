//! Architectural exception and redundant-prefix coverage shared by legacy SHA-NI.

use crate::common::{run_until_hlt, setup_vm, setup_vm_no_idt};
use rax::vm::vcpu::{Registers, VCpu, VcpuExit};

const SHA_REGISTER_FORMS: &[(&str, &[u8])] = &[
    ("SHA1NEXTE", &[0x0F, 0x38, 0xC8, 0xC1]),
    ("SHA1MSG1", &[0x0F, 0x38, 0xC9, 0xC1]),
    ("SHA1MSG2", &[0x0F, 0x38, 0xCA, 0xC1]),
    ("SHA256RNDS2", &[0x0F, 0x38, 0xCB, 0xC1]),
    ("SHA256MSG1", &[0x0F, 0x38, 0xCC, 0xC1]),
    ("SHA256MSG2", &[0x0F, 0x38, 0xCD, 0xC1]),
    ("SHA1RNDS4", &[0x0F, 0x3A, 0xCC, 0xC1, 0x03]),
];

const SHA_MEMORY_FORMS: &[(&str, &[u8])] = &[
    ("SHA1NEXTE", &[0x0F, 0x38, 0xC8, 0x00]),
    ("SHA1MSG1", &[0x0F, 0x38, 0xC9, 0x00]),
    ("SHA1MSG2", &[0x0F, 0x38, 0xCA, 0x00]),
    ("SHA256RNDS2", &[0x0F, 0x38, 0xCB, 0x00]),
    ("SHA256MSG1", &[0x0F, 0x38, 0xCC, 0x00]),
    ("SHA256MSG2", &[0x0F, 0x38, 0xCD, 0x00]),
    ("SHA1RNDS4", &[0x0F, 0x3A, 0xCC, 0x00, 0x03]),
];

#[test]
fn sha_ni_redundant_legacy_prefixes_remain_accepted() {
    for &(name, instruction) in SHA_REGISTER_FORMS {
        for prefix in [None, Some(0x66), Some(0xF2), Some(0xF3)] {
            let mut code = Vec::with_capacity(instruction.len() + 2);
            code.extend(prefix);
            code.extend_from_slice(instruction);
            code.push(0xF4);
            let (mut vcpu, _) = setup_vm(&code, None);
            run_until_hlt(&mut vcpu)
                .unwrap_or_else(|error| panic!("{name} prefix {prefix:02X?}: {error}"));
        }
    }
}

#[test]
fn sha_ni_misaligned_memory_sources_raise_gp_before_reading() {
    for &(name, instruction) in SHA_MEMORY_FORMS {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let mut regs = Registers::default();
        // Unmapped and misaligned: Type-4 exception ordering requires #GP(0)
        // before the source read could produce a page/memory fault.
        regs.rax = 0xDEAD_0001;
        let (mut vcpu, _) = setup_vm_no_idt(&code, Some(regs));
        match vcpu.run() {
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("vector 13")
                        || message.contains("IDT entry 13")
                        || message.contains("#GP"),
                    "{name} must inject #GP(0), got {message}",
                );
            }
            Ok(VcpuExit::Hlt) => panic!("{name} accepted a misaligned m128 source"),
            Ok(other) => panic!("{name} must inject #GP(0), got {other:?}"),
        }
    }
}
