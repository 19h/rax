//! Fail-closed RVV validation through the x86-64 native helper ABI.

use super::*;

fn run_invalid_vector_case(instruction: u32, initial: RiscVGuestRegs) {
    let bytes = instruction.to_le_bytes();
    let mut lifter = RiscVLifter::rv64gc();
    let mut context = LiftContext::new(SourceArch::RiscV64);
    let lifted = lifter
        .lift_insn(CODE, &bytes, &mut context)
        .expect("lift reserved vector encoding");
    let (function, return_pcs) =
        function_for_lift(lifted.control_flow, lifted.ops, lifted.bytes_consumed);

    for level in [OptLevel::O0, OptLevel::O2] {
        let mut optimized = function.clone();
        optimize_function(&mut optimized, level);
        let mut lowerer = RiscVX86_64Lowerer::new();
        lowerer.set_return_pcs(return_pcs.clone());
        let lowered = lowerer.lower_function(&optimized).unwrap_or_else(|error| {
            panic!("lower reserved vector encoding at {level:?}: {error:?}")
        });
        let code = lowerer.finalize().expect("finalize reserved vector code");
        let executable = ExecMem::new(&code).expect("map reserved vector code");

        let initial_memory = [0xa5; MEMORY_LEN];
        let mut memory = TestMemory::new(initial_memory);
        let mut state = jit_state(&mut memory, initial.x, initial.f, initial.fcsr as u32, CODE);
        state.v = initial.v;
        state.vl = initial.vl;
        state.vtype = initial.vtype;
        state.vstart = initial.vstart;
        state.vcsr = initial.vcsr;
        let mut expected = state;
        expected.exit_reason = 1;

        executable.run_riscv(lowered.entry_offset, &mut state);

        assert_eq!(state, expected, "partial state commit at {level:?}");
        assert_eq!(memory.bytes, initial_memory, "memory commit at {level:?}");
    }
}

#[test]
fn lifted_rv_vector_reserved_encodings_fail_closed_transactionally() {
    let mut initial = RiscVGuestRegs {
        vl: 4,
        vtype: 0x10, // e32,m1
        vcsr: 5,
        ..Default::default()
    };
    for register in 1..32usize {
        initial.x[register] = 0x1020_3040_5060_7080 ^ register as u64;
    }
    for register in 0..32usize {
        initial.f[register] = 0xffff_ffff_3f80_0000 + register as u64;
        initial.v[register] = [register as u8; 16];
    }

    let cases = [
        // vmsbf.m v2,v2: destination/source overlap.
        (
            (0b010100 << 26)
                | (1 << 25)
                | (2 << 20)
                | (0b00001 << 15)
                | (0b010 << 12)
                | (2 << 7)
                | 0x57,
            initial,
        ),
        // vadc.vvm v0,v2,v3: v0 cannot be the destination.
        (
            (0b010000 << 26) | (2 << 20) | (3 << 15) | (0 << 12) | 0x57,
            initial,
        ),
        // vslide1up.vx v2,v2,x3: exact encoding and overlapping groups.
        (
            (0b001110 << 26) | (1 << 25) | (2 << 20) | (3 << 15) | (0b110 << 12) | (2 << 7) | 0x57,
            initial,
        ),
        // vnsrl.wv v3,v2,v4: destination overlaps the upper half of wide vs2.
        (
            (0b101100 << 26) | (1 << 25) | (2 << 20) | (4 << 15) | (3 << 7) | 0x57,
            initial,
        ),
        // vwadd.vv v0,v0,v2: narrow vs2 overlaps the low part of wide vd.
        (
            (0b110000 << 26) | (1 << 25) | (2 << 15) | (0b010 << 12) | 0x57,
            initial,
        ),
        // vwadd.vv v1,v1,v2 under mf2: source EMUL is below one, so even
        // same-register overlap is reserved.
        (
            (0b110000 << 26) | (1 << 25) | (1 << 20) | (2 << 15) | (0b010 << 12) | (1 << 7) | 0x57,
            RiscVGuestRegs {
                vtype: 0x17, // e32,mf2
                ..initial
            },
        ),
        // vfwadd.vv v0,v0,v2: FP widening uses the same group rule.
        (
            (0b110000 << 26) | (1 << 25) | (2 << 15) | (0b001 << 12) | 0x57,
            initial,
        ),
        // vzext.vf2 v0,v0 under m2: source overlaps the low part of vd.
        (
            (0b010010 << 26) | (1 << 25) | (0b00110 << 15) | (0b010 << 12) | 0x57,
            RiscVGuestRegs {
                vtype: 0x11, // e32,m2
                ..initial
            },
        ),
        // vfadd.vv at SEW=8 would consume unsupported FP8 operands.
        (
            (1 << 25) | (2 << 20) | (3 << 15) | (0b001 << 12) | (1 << 7) | 0x57,
            RiscVGuestRegs {
                vtype: 0x00, // e8,m1
                ..initial
            },
        ),
        // vfsgnj.vv with frm=7 and vl=0 must still reject before execution.
        (
            (0b001000 << 26) | (1 << 25) | (2 << 20) | (3 << 15) | (0b001 << 12) | (1 << 7) | 0x57,
            RiscVGuestRegs {
                fcsr: 7 << 5,
                vl: 0,
                ..initial
            },
        ),
        // vid.v reserves vs2 and requires it to encode v0.
        (
            (0b010100 << 26)
                | (1 << 25)
                | (3 << 20)
                | (0b10001 << 15)
                | (0b010 << 12)
                | (1 << 7)
                | 0x57,
            RiscVGuestRegs {
                vtype: 0, // e8,m1
                ..initial
            },
        ),
    ];

    for (instruction, state) in cases {
        run_invalid_vector_case(instruction, state);
    }
}

#[test]
fn lifted_vid_with_vs2_v0_remains_legal_at_o0_and_o2() {
    let instruction =
        (0b010100 << 26) | (1 << 25) | (0b10001 << 15) | (0b010 << 12) | (1 << 7) | 0x57;
    let initial = RiscVGuestRegs {
        vl: 4,
        vtype: 0, // e8,m1
        ..Default::default()
    };
    run_vector_case(instruction, initial, [0xa5; MEMORY_LEN], false);
}

#[test]
fn lifted_vfncvt_fp16_to_integer8_matches_direct_at_o0_and_o2() {
    let mut initial = RiscVGuestRegs {
        vl: 4,
        vtype: 0, // e8,m1
        ..Default::default()
    };
    for (lane, bits) in [0x3e00u16, 0xbe00, 0x5c00, 0x7e00].into_iter().enumerate() {
        initial.v[2][lane * 2..lane * 2 + 2].copy_from_slice(&bits.to_le_bytes());
    }

    for selector in [0b10000, 0b10001, 0b10110, 0b10111] {
        let instruction = (0b010010 << 26)
            | (1 << 25)
            | (2 << 20)
            | (selector << 15)
            | (0b001 << 12)
            | (1 << 7)
            | 0x57;
        run_vector_case(instruction, initial, [0xa5; MEMORY_LEN], false);
    }
}
