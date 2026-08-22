//! XLEN-specific Zfa moves between integer and floating-point registers.

use super::*;

pub(super) fn decode_zfa_rv32_move(
    funct7: u32,
    funct3: u8,
    rs2: u8,
    rv64: bool,
    isa: &Isa,
) -> Option<Op> {
    if rv64 || !isa.d {
        return None;
    }
    match (funct7, funct3, rs2) {
        (0b1110001, 0, 1) => Some(Op::FmvhXD),
        (0b1011001, 0, _) => Some(Op::FmvpDX),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(funct7: u32, rs2: u32, rs1: u32, rd: u32) -> u32 {
        (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (rd << 7) | 0x53
    }

    #[test]
    fn doubleword_moves_require_rv32_d_and_zfa() {
        let fmvh_x_d = enc(0b1110001, 1, 10, 11);
        let fmvp_d_x = enc(0b1011001, 12, 11, 10);
        let full = Isa::rv64gc();

        assert_eq!(decode(fmvh_x_d, Xlen::Rv32, &full).op, Op::FmvhXD);
        assert_eq!(decode(fmvp_d_x, Xlen::Rv32, &full).op, Op::FmvpDX);
        assert!(decode(fmvh_x_d, Xlen::Rv64, &full).is_illegal());
        assert!(decode(fmvp_d_x, Xlen::Rv64, &full).is_illegal());

        let mut no_d = full;
        no_d.d = false;
        assert!(decode(fmvh_x_d, Xlen::Rv32, &no_d).is_illegal());
        assert!(decode(fmvp_d_x, Xlen::Rv32, &no_d).is_illegal());

        let mut no_zfa = full;
        no_zfa.zfa = false;
        assert!(decode(fmvh_x_d, Xlen::Rv32, &no_zfa).is_illegal());
        assert!(decode(fmvp_d_x, Xlen::Rv32, &no_zfa).is_illegal());
    }
}
