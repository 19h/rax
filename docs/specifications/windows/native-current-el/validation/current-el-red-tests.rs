//! Native Windows ARM64 CurrentEL compatibility at the scheduler boundary.
use super::*;

fn instruction(arch: WinArch, word: u32) -> (Proc, Thread, u64) {
    let mut p = process(arch);
    let (pc, _) =
        p.vm.allocate(None, PAGE_SIZE, mem::RESERVE | mem::COMMIT, prot::READWRITE)
            .unwrap();
    p.space.w32(pc, word).unwrap();
    p.vm.protect(pc, PAGE_SIZE, prot::EXECUTE_READ).unwrap();
    let mut t = thread(&mut p, 8);
    t.cpu.set_pc(pc);
    (p, t, pc)
}

#[test]
fn windows_current_el_read_retires_all_destinations_and_preserves_context() {
    for rt in 0..32 {
        let word = 0xD538_4240 | rt;
        let (mut p, mut t, pc) = instruction(WinArch::Arm64, word);
        for n in 0..31 {
            t.cpu.set_gpr(n, 0x1234_5678_9000_0000 + n as u64);
        }
        t.cpu.set_teb(t.teb);
        t.cpu.a64_mut().unwrap().core_mut().set_nzcv_bits(0b1011);
        let before = (0..31).map(|n| t.cpu.gpr(n)).collect::<Vec<_>>();
        let sp = t.cpu.sp();
        let stop = t.cpu.run(1);
        assert!(matches!(stop, CpuStop::Undefined { pc: at, .. } if at == pc));
        assert_eq!(
            handle_stop(&mut p, &mut t, stop),
            Outcome::Continue,
            "rt={rt}"
        );
        assert_eq!(t.cpu.pc(), pc + 4, "rt={rt}");
        for (n, expected) in before.into_iter().enumerate() {
            let expected = if n == rt as usize && n != 18 {
                0
            } else {
                expected
            };
            assert_eq!(t.cpu.gpr(n), expected, "rt={rt} x{n}");
        }
        assert_eq!(t.cpu.sp(), sp);
        assert_eq!(t.cpu.a64().unwrap().core().nzcv_bits(), 0b1011);
        assert_eq!(p.space.u32(pc).unwrap(), word);
        assert!(t.frames.is_empty());
        assert!(p.failure.is_none());
    }
}

#[test]
fn windows_current_el_compatibility_does_not_admit_other_privileged_instructions() {
    for word in [
        0xD538_1000, // MRS X0,SCTLR_EL1.
        0xD538_2000, // MRS X0,TTBR0_EL1.
        0xD538_4200, // MRS X0,SPSel.
        0xD538_4100, // MRS X0,SP_EL0.
        0xD518_4240, // MSR CurrentEL,X0 (reserved write).
        0,           // UDF #0.
    ] {
        let (mut p, mut t, pc) = instruction(WinArch::Arm64, word);
        let stop = t.cpu.run(1);
        assert!(matches!(stop, CpuStop::Undefined { pc: at, .. } if at == pc));
        assert_eq!(
            handle_stop(&mut p, &mut t, stop),
            Outcome::ProcessTerminate(STATUS_ILLEGAL_INSTRUCTION),
            "word={word:#010x}"
        );
        assert_eq!(t.cpu.pc(), pc);
    }
}

#[test]
fn shared_arm64_core_still_rejects_current_el_at_el0_for_every_destination() {
    for rt in 0..32 {
        let (_p, mut t, pc) = instruction(WinArch::Arm64, 0xD538_4240 | rt);
        t.cpu.set_gpr(0, 0xDEAD_BEEF);
        assert!(matches!(t.cpu.run(1), CpuStop::Undefined { pc: at, .. } if at == pc));
        assert_eq!(t.cpu.pc(), pc);
        assert_eq!(t.cpu.gpr(0), 0xDEAD_BEEF);
    }
}
