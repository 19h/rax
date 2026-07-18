//! tests::branch tests

use super::*;
use crate::isa::arm::aarch64::cpu::*;

    // -------------------------------------------------------------------------
    // Data Processing Immediate - PC-relative addressing
    // -------------------------------------------------------------------------

    #[test]
    fn test_adr() {
        // ADR X0, #0x100 (PC + 0x100)
        // ADR: [0 immlo[1:0] 10000 immhi[18:0] Rd[4:0]]
        // PC=0, imm=0x100 -> immhi=0x40, immlo=0
        let insn = 0x10000800; // ADR X0, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x100);
        assert_eq!(cpu.get_pc(), 4);
    }
    #[test]
    fn test_adrp() {
        // ADRP X1, #0x1000 (page-aligned, PC + 0x1000)
        // ADRP: [1 immlo[1:0] 10000 immhi[18:0] Rd[4:0]]
        let insn = 0x90000001; // ADRP X1, #0 (current page)
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(1), 0); // Page of PC=0
        assert_eq!(cpu.get_pc(), 4);
    }
    // -------------------------------------------------------------------------
    // Branch Instructions - Conditional
    // -------------------------------------------------------------------------

    #[test]
    fn test_b_cond_taken() {
        // B.EQ #0x100 (taken when Z=1)
        let insn = 0x54000800; // B.EQ #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(true);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x100);
    }
    #[test]
    fn test_b_cond_not_taken() {
        // B.EQ #0x100 (not taken when Z=0)
        let insn = 0x54000800; // B.EQ #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(false);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4); // Falls through
    }
    #[test]
    fn test_b_ne() {
        // B.NE #0x20
        let insn = 0x54000101; // B.NE #0x20
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(false);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x20);
    }
    // -------------------------------------------------------------------------
    // Branch Instructions - Unconditional
    // -------------------------------------------------------------------------

    #[test]
    fn test_b() {
        // B #0x1000
        let insn = 0x14000400; // B #0x1000
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x1000);
    }
    #[test]
    fn test_b_negative() {
        // B #-0x100 (backward branch)
        // imm26 = -0x40 (in instruction words) = 0x3FFFFC0
        let insn = 0x17FFFFC0; // B #-0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_pc(0x1000);
        write_insn(&mut cpu, 0x1000, insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0xF00);
    }
    #[test]
    fn test_bl() {
        // BL #0x100 (saves return address in X30)
        let insn = 0x94000040; // BL #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x100);
        assert_eq!(cpu.get_x(30), 4); // Return address
    }
    // -------------------------------------------------------------------------
    // Branch Instructions - Compare and Branch
    // -------------------------------------------------------------------------

    #[test]
    fn test_cbz_taken() {
        // CBZ X0, #0x100
        let insn = 0xB4000800; // CBZ X0, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x100);
    }
    #[test]
    fn test_cbz_not_taken() {
        // CBZ X0, #0x100
        let insn = 0xB4000800; // CBZ X0, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 1);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4);
    }
    #[test]
    fn test_cbnz_taken() {
        // CBNZ X1, #0x80
        let insn = 0xB5000401; // CBNZ X1, #0x80
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1234);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x80);
    }
    #[test]
    fn test_cbz_32bit() {
        // CBZ W0, #0x20
        let insn = 0x34000100; // CBZ W0, #0x20
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xFFFF_FFFF_0000_0000); // Upper bits set but W0 is 0
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x20);
    }
    // -------------------------------------------------------------------------
    // Branch Instructions - Test and Branch
    // -------------------------------------------------------------------------

    #[test]
    fn test_tbz_taken() {
        // TBZ X0, #0, #0x40 (branch if bit 0 is 0)
        let insn = 0x36000200; // TBZ X0, #0, #0x40
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xFFFE); // Bit 0 is 0
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x40);
    }
    #[test]
    fn test_tbz_not_taken() {
        // TBZ X0, #0, #0x40
        let insn = 0x36000200; // TBZ X0, #0, #0x40
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xFFFF); // Bit 0 is 1
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4);
    }
    #[test]
    fn test_tbnz_taken() {
        // TBNZ X0, #4, #0x80 (branch if bit 4 is 1)
        let insn = 0x37200400; // TBNZ X0, #4, #0x80
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x10); // Bit 4 is 1
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x80);
    }
    #[test]
    fn test_tbz_high_bit() {
        // TBZ X0, #63, #0x20 (test highest bit)
        let insn = 0xB6F80100; // TBZ X0, #63, #0x20
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x7FFF_FFFF_FFFF_FFFF); // Bit 63 is 0
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x20);
    }
    #[test]
    fn test_blr() {
        // BLR X5
        let insn = 0xD63F00A0; // BLR X5
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(5, 0x4000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x4000);
        assert_eq!(cpu.get_x(30), 4); // Return address
    }
    #[test]
    fn test_ret() {
        // RET (uses X30 by default)
        let insn = 0xD65F03C0; // RET
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(30, 0x8000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x8000);
    }
    #[test]
    fn test_ret_xn() {
        // RET X5
        let insn = 0xD65F00A0; // RET X5
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(5, 0x3000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x3000);
    }
