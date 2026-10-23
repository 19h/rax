//! Architectural observability at Hexagon packet boundaries.
use crate::smir::ir::types::{ArchReg, HexagonReg, VReg};

pub(super) fn boundary_sources() -> Vec<VReg> {
    let mut result = Vec::new();
    // A packet boundary observes the complete architectural file.
    // These implicit reads anchor writes across a later packet fault.
    result.extend((0..32).map(|n| VReg::Arch(ArchReg::Hexagon(HexagonReg::R(n)))));
    result.extend((0..4).map(|n| VReg::Arch(ArchReg::Hexagon(HexagonReg::P(n)))));
    result.extend((0..32).map(|n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)))));
    result.extend((0..4).map(|n| VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(n)))));
    for register in [
        HexagonReg::Pc,
        HexagonReg::Gp,
        HexagonReg::Sp,
        HexagonReg::Fp,
        HexagonReg::Lr,
        HexagonReg::Usr,
        HexagonReg::Lc0,
        HexagonReg::Lc1,
        HexagonReg::Sa0,
        HexagonReg::Sa1,
        HexagonReg::M(0),
        HexagonReg::M(1),
        HexagonReg::Cs(0),
        HexagonReg::Cs(1),
    ] {
        result.push(VReg::Arch(ArchReg::Hexagon(register)));
    }
    result
}
