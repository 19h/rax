//! Strict-lift coverage for fixed-predicate VEX packed-integer comparisons.

use super::*;

fn memory_address(bytes: &[u8]) -> Address {
    let result = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let [
        SmirOp {
            kind: OpKind::VLoad { addr, .. },
            ..
        },
        SmirOp {
            kind: OpKind::VCmp { .. },
            ..
        },
    ] = result.ops.as_slice()
    else {
        panic!(
            "{bytes:02X?}: expected exact VLoad/VCmp pair, got {:?}",
            result.ops
        );
    };
    addr.clone()
}

#[test]
fn vex_integer_compare_preserves_addr32_and_segment_prefixes() {
    // VPCMPEQB xmm0, xmm1, xmmword ptr [rax].
    assert_eq!(
        memory_address(&[0xC5, 0xF1, 0x74, 0x00]),
        Address::Direct(x86_gpr(0)),
    );
    assert_eq!(
        memory_address(&[0x67, 0xC5, 0xF1, 0x74, 0x00]),
        Address::X86Addr32(Box::new(Address::Direct(x86_gpr(0)))),
    );
    assert_eq!(
        memory_address(&[0x64, 0xC5, 0xF1, 0x74, 0x00]),
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            base: Some(x86_gpr(0)),
            index: None,
            scale: 1,
            disp: 0,
        },
    );
    assert_eq!(
        memory_address(&[0x65, 0x67, 0xC5, 0xF1, 0x74, 0x00]),
        Address::X86Addr32(Box::new(Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
            base: Some(x86_gpr(0)),
            index: None,
            scale: 1,
            disp: 0,
        })),
    );
}
