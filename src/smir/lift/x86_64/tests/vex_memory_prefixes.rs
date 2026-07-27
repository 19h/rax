//! Effective-address prefix coverage for standalone VEX lifters.

use super::*;

const MEMORY_CASES: &[(&str, &[u8])] = &[
    ("VPERM2I128", &[0xC4, 0xE3, 0x7D, 0x46, 0x00, 0x82]),
    ("VPDPBUUD", &[0xC4, 0xE2, 0x78, 0x50, 0x00]),
    ("VPUNPCKLBW", &[0xC5, 0xF1, 0x60, 0x00]),
    ("VPACKSSWB", &[0xC5, 0xF1, 0x63, 0x00]),
    ("VPSHUFB", &[0xC4, 0xE2, 0x71, 0x00, 0x00]),
    ("VPHADDW", &[0xC4, 0xE2, 0x71, 0x01, 0x00]),
    ("VPMADDUBSW", &[0xC4, 0xE2, 0x71, 0x04, 0x00]),
    ("VPSIGNB", &[0xC4, 0xE2, 0x71, 0x08, 0x00]),
    ("VPMULHRSW", &[0xC4, 0xE2, 0x71, 0x0B, 0x00]),
    ("VPABSB", &[0xC4, 0xE2, 0x79, 0x1C, 0x00]),
    ("VPTEST", &[0xC4, 0xE2, 0x79, 0x17, 0x00]),
    ("VTESTPS", &[0xC4, 0xE2, 0x79, 0x0E, 0x00]),
    ("VPHMINPOSUW", &[0xC4, 0xE2, 0x79, 0x41, 0x00]),
    ("VSM3MSG1", &[0xC4, 0xE2, 0x78, 0xDA, 0x00]),
    ("VSM3RNDS2", &[0xC4, 0xE3, 0x79, 0xDE, 0x00, 0x00]),
    ("VSM4KEY4", &[0xC4, 0xE2, 0x7A, 0xDA, 0x00]),
    ("VCVTNEEBF162PS", &[0xC4, 0xE2, 0x7A, 0xB0, 0x00]),
    ("VDPPS", &[0xC4, 0xE3, 0x79, 0x40, 0x00, 0x00]),
    ("VROUNDPS", &[0xC4, 0xE3, 0x79, 0x08, 0x00, 0x00]),
];

fn with_legacy_prefixes(prefixes: &[u8], instruction: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(prefixes.len() + instruction.len());
    bytes.extend_from_slice(prefixes);
    bytes.extend_from_slice(instruction);
    bytes
}

fn memory_address(name: &str, bytes: &[u8]) -> Address {
    let result =
        lift_single(bytes).unwrap_or_else(|error| panic!("{name} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{name} {bytes:02X?}");

    let addresses = result
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => Some(addr),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        addresses.len(),
        1,
        "{name} {bytes:02X?}: expected one memory-source load, got {:?}",
        result.ops
    );
    addresses[0].clone()
}

fn segment_address(segment: X86Reg) -> Address {
    Address::SegmentRel {
        segment: VReg::Arch(ArchReg::X86(segment)),
        base: Some(x86_gpr(0)),
        index: None,
        scale: 1,
        disp: 0,
    }
}

#[test]
fn standalone_vex_memory_families_preserve_address_size_and_segment_prefixes() {
    for &(name, instruction) in MEMORY_CASES {
        assert_eq!(
            memory_address(name, instruction),
            Address::Direct(x86_gpr(0)),
            "{name}: default [rax]"
        );
        assert_eq!(
            memory_address(name, &with_legacy_prefixes(&[0x67], instruction)),
            Address::X86Addr32(Box::new(Address::Direct(x86_gpr(0)))),
            "{name}: addr32 [eax]"
        );
        assert_eq!(
            memory_address(name, &with_legacy_prefixes(&[0x64], instruction)),
            segment_address(X86Reg::FsBase),
            "{name}: fs:[rax]"
        );
        assert_eq!(
            memory_address(name, &with_legacy_prefixes(&[0x65, 0x67], instruction)),
            Address::X86Addr32(Box::new(segment_address(X86Reg::GsBase))),
            "{name}: gs:[eax]"
        );
    }
}
