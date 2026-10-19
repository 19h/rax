//! eBPF (IETF RFC 9669 / Linux kernel v1–v4) and Solana Bytecode Format
//! (SBPF V0–V4) instruction set architecture implementation.

mod arith;
pub mod cpu;
pub mod decode;
pub mod disasm;
pub mod memory;
pub mod platform;
pub mod verifier;

pub use cpu::{EbpfCallFrame, EbpfConfig, EbpfCpu, EbpfExit, EbpfRegisters, EbpfTrap, EbpfVcpu};
pub use decode::{
    AluWidth, AtomicKind, BinAluOp, EbpfDecodeError, EbpfDialect, EbpfInsn, EbpfMemWidth, EbpfOp,
    EbpfSrc, EndianOp, FRAME_PTR_REG, INSN_SIZE, JmpCond, SbpfVersion, WIDE_INSN_SIZE, decode_insn,
};
pub use memory::{
    EbpfMemFault, EbpfMemory, MM_BYTECODE_START, MM_HEAP_START, MM_INPUT_START, MM_REGION_SIZE,
    MM_RODATA_START, MM_STACK_START, SbpfMemoryMapping, SbpfMemoryRegion, VIRTUAL_ADDRESS_BITS,
};
pub use platform::{BTF_HELPER_SYSCALL_KEY, EbpfHelperKey, EbpfMap, EbpfPlatform};
pub use verifier::{SbpfVerifierError, verify_sbpf};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::memory::FlatMemory;
    use crate::smir::ir::types::Endian;

    fn encode_basic(opc: u8, dst: u8, src: u8, off: i16, imm: i32, endian: Endian) -> [u8; 8] {
        let regs = match endian {
            Endian::Little => (src << 4) | (dst & 0x0f),
            Endian::Big => (dst << 4) | (src & 0x0f),
        };
        let off_b = match endian {
            Endian::Little => off.to_le_bytes(),
            Endian::Big => off.to_be_bytes(),
        };
        let imm_b = match endian {
            Endian::Little => imm.to_le_bytes(),
            Endian::Big => imm.to_be_bytes(),
        };
        [
            opc, regs, off_b[0], off_b[1], imm_b[0], imm_b[1], imm_b[2], imm_b[3],
        ]
    }

    #[test]
    fn decode_encode_roundtrip_rfc9669_le_and_be() {
        let samples: &[(u8, u8, u8, i16, i32)] = &[
            (0x07, 1, 0, 0, 42),   // add64 r1, 42
            (0x0f, 2, 3, 0, 0),    // add64 r2, r3
            (0x37, 4, 0, 1, -7),   // sdiv64 r4, -7
            (0x97, 5, 0, 1, 3),    // smod64 r5, 3
            (0xbf, 6, 7, 8, 0),    // movsx64 r6, r7, 8
            (0xbf, 6, 7, 16, 0),   // movsx64 r6, r7, 16
            (0xbf, 6, 7, 32, 0),   // movsx64 r6, r7, 32
            (0xbc, 6, 7, 8, 0),    // movsx32 r6, r7, 8
            (0xd7, 8, 0, 0, 16),   // bswap16 r8
            (0xd7, 8, 0, 0, 32),   // bswap32 r8
            (0xd7, 8, 0, 0, 64),   // bswap64 r8
            (0x91, 1, 2, -8, 0),   // ldsxb r1, [r2-8]
            (0x89, 1, 2, 16, 0),   // ldsxh r1, [r2+16]
            (0x81, 1, 2, 32, 0),   // ldsxw r1, [r2+32]
            (0x06, 0, 0, 0, 128),  // ja32 +128
            (0xdb, 1, 2, 8, 0x01), // lock *(u64 *)(r1+8) += r2 (fetch)
            (0xdb, 1, 2, 8, 0xe1), // r2 = xchg_64(r1+8, r2)
            (0xdb, 1, 2, 8, 0xf1), // r0 = cmpxchg_64(r1+8, r0, r2)
            (0x95, 0, 0, 0, 0),    // exit
        ];

        for endian in [Endian::Little, Endian::Big] {
            for &(opc, dst, src, off, imm) in samples {
                let bytes = encode_basic(opc, dst, src, off, imm, endian);
                let decoded = decode_insn(&bytes, EbpfDialect::Rfc9669, endian)
                    .unwrap_or_else(|e| panic!("failed to decode 0x{opc:02x} ({endian:?}): {e}"));
                assert_eq!(decoded.encode(endian), bytes);
                assert!(!decoded.mnemonic().is_empty());
                assert!(!decoded.disassemble().is_empty());
            }
        }
    }

    #[test]
    fn rfc9669_div_mod_sdiv_smod_edge_cases() {
        let mut prog = Vec::new();
        // r0 = 0x1_0000_0005 / 0 (div64 by 0 -> 0)
        prog.extend_from_slice(&encode_basic(0x3f, 0, 1, 0, 0, Endian::Little));
        // r2 = 0x1_0000_0005 % 0 (mod64 by 0 -> unchanged)
        prog.extend_from_slice(&encode_basic(0x9f, 2, 1, 0, 0, Endian::Little));
        // r3 = (u32)0x1_0000_0005 % 0 (mod32 by 0 -> zero-extended low 32 bits = 5)
        prog.extend_from_slice(&encode_basic(0x9c, 3, 1, 0, 0, Endian::Little));
        // r4 = i64::MIN /s -1 (sdiv64 overflow -> i64::MIN)
        prog.extend_from_slice(&encode_basic(0x37, 4, 0, 1, -1, Endian::Little));
        // r5 = i64::MIN %s -1 (smod64 overflow -> 0)
        prog.extend_from_slice(&encode_basic(0x97, 5, 0, 1, -1, Endian::Little));
        // r6 = -13 /s 3 -> -4, r7 = -13 %s 3 -> -1
        prog.extend_from_slice(&encode_basic(0x37, 6, 0, 1, 3, Endian::Little));
        prog.extend_from_slice(&encode_basic(0x97, 7, 0, 1, 3, Endian::Little));
        prog.extend_from_slice(&encode_basic(0x95, 0, 0, 0, 0, Endian::Little));

        let mut mem = FlatMemory::new(0x10000);
        mem.load(0, &prog);

        let mut cpu = EbpfCpu::rfc9669();
        cpu.regs.r[0] = 0x1_0000_0005;
        cpu.regs.r[1] = 0;
        cpu.regs.r[2] = 0x1_0000_0005;
        cpu.regs.r[3] = 0x1_0000_0005;
        cpu.regs.r[4] = i64::MIN as u64;
        cpu.regs.r[5] = i64::MIN as u64;
        cpu.regs.r[6] = (-13i64) as u64;
        cpu.regs.r[7] = (-13i64) as u64;

        let exit = cpu.run(&mut mem);
        assert_eq!(exit, EbpfExit::Exit { r0: 0 });
        assert_eq!(cpu.regs.r[2], 0x1_0000_0005);
        assert_eq!(cpu.regs.r[3], 5);
        assert_eq!(cpu.regs.r[4], i64::MIN as u64);
        assert_eq!(cpu.regs.r[5], 0);
        assert_eq!(cpu.regs.r[6], (-4i64) as u64);
        assert_eq!(cpu.regs.r[7], (-1i64) as u64);
    }

    #[test]
    fn solana_sbpf_v0_to_v4_dialect_differences() {
        // 1. ADD32_IMM sign-extends in V0/V1/V3/V4, zero-extends in V2
        let add32_neg1 = encode_basic(0x04, 1, 0, 0, -1, Endian::Little);
        let exit_insn = encode_basic(0x95, 0, 0, 0, 0, Endian::Little);
        let mut prog = Vec::new();
        prog.extend_from_slice(&add32_neg1);
        prog.extend_from_slice(&exit_insn);

        for (ver, expected_r1) in [
            (SbpfVersion::V0, u64::MAX),
            (SbpfVersion::V1, u64::MAX),
            (SbpfVersion::V2, u64::from(u32::MAX)),
            (SbpfVersion::V3, u64::MAX),
            (SbpfVersion::V4, u64::MAX),
        ] {
            let mut mem = FlatMemory::new(0x10000);
            mem.load(0, &prog);
            let mut cpu = EbpfCpu::solana(ver);
            cpu.regs.r[1] = 0;
            cpu.run(&mut mem);
            assert_eq!(cpu.regs.r[1], expected_r1, "ADD32_IMM mismatch on {ver:?}");
        }

        // 2. SUB64_IMM swaps operands in V2 (`dst = imm - dst`), normal in V0/V1/V3/V4 (`dst = dst - imm`)
        let sub64_imm = encode_basic(0x17, 1, 0, 0, 100, Endian::Little);
        let mut prog_sub = Vec::new();
        prog_sub.extend_from_slice(&sub64_imm);
        prog_sub.extend_from_slice(&exit_insn);

        for (ver, expected_r1) in [
            (SbpfVersion::V1, 30u64.wrapping_sub(100)),
            (SbpfVersion::V2, 100u64.wrapping_sub(30)),
            (SbpfVersion::V3, 30u64.wrapping_sub(100)),
        ] {
            let mut mem = FlatMemory::new(0x10000);
            mem.load(0, &prog_sub);
            let mut cpu = EbpfCpu::solana(ver);
            cpu.regs.r[1] = 30;
            cpu.run(&mut mem);
            assert_eq!(cpu.regs.r[1], expected_r1, "SUB64_IMM mismatch on {ver:?}");
        }

        // 3. SBPF V2 PQR class (UHMUL64, SHMUL64, SDIV64 overflow trap)
        let mut prog_v2 = Vec::new();
        // HOR64_IMM r1, 0x1234_5678
        prog_v2.extend_from_slice(&encode_basic(0xf7, 1, 0, 0, 0x1234_5678, Endian::Little));
        // UHMUL64_REG r2, r3
        prog_v2.extend_from_slice(&encode_basic(0x3e, 2, 3, 0, 0, Endian::Little));
        // SHMUL64_IMM r4, -2
        prog_v2.extend_from_slice(&encode_basic(0xb6, 4, 0, 0, -2, Endian::Little));
        prog_v2.extend_from_slice(&exit_insn);

        let mut mem_v2 = FlatMemory::new(0x10000);
        mem_v2.load(0, &prog_v2);
        let mut cpu_v2 = EbpfCpu::solana(SbpfVersion::V2);
        cpu_v2.regs.r[1] = 0x9abc_def0;
        cpu_v2.regs.r[2] = u64::MAX;
        cpu_v2.regs.r[3] = 2;
        cpu_v2.regs.r[4] = i64::MIN as u64;
        cpu_v2.run(&mut mem_v2);
        assert_eq!(cpu_v2.regs.r[1], 0x1234_5678_9abc_def0);
        assert_eq!(cpu_v2.regs.r[2], 1);
        assert_eq!(cpu_v2.regs.r[4], 1);
    }

    #[test]
    fn vcpu_adapter_steps_exits_resumes_traps_and_round_trips_state() {
        use crate::vm::vcpu::{CpuState, VCpu, VcpuExit};
        use std::sync::Arc;
        use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

        let le = Endian::Little;
        // r0 = 40; call 77 (unregistered helper); r0 += 2; exit
        let program = [
            encode_basic(0xb7, 0, 0, 0, 40, le),
            encode_basic(0x85, 0, 0, 0, 77, le),
            encode_basic(0x07, 0, 0, 0, 2, le),
            encode_basic(0x95, 0, 0, 0, 0, le),
        ]
        .concat();
        let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x1000)]).unwrap());
        memory.write_slice(&program, GuestAddress(0)).unwrap();
        let mut vcpu = EbpfVcpu::new(3, memory.clone(), EbpfConfig::default());
        assert_eq!((VCpu::id(&vcpu), vcpu.id()), (3, 3));
        assert!(vcpu.supports_stepping());

        // One step executes one guest instruction.
        assert!(matches!(vcpu.step_insn(), Ok(None)));
        assert_eq!((vcpu.current_pc(), vcpu.instruction_count()), (8, 1));
        // The unregistered helper exits at its continuation; the host
        // supplies r0 and resumes.
        assert!(matches!(vcpu.run(), Ok(VcpuExit::SystemCall)));
        assert_eq!((vcpu.current_pc(), vcpu.instruction_count()), (16, 2));
        vcpu.cpu_mut().regs.r[0] = 40;
        assert!(matches!(vcpu.run(), Ok(VcpuExit::Shutdown)));
        assert_eq!((vcpu.cpu().regs.r[0], vcpu.instruction_count()), (42, 4));
        // A halted vCPU reports Hlt until woken.
        assert!(matches!(vcpu.run(), Ok(VcpuExit::Hlt)));
        assert!(matches!(vcpu.step_insn(), Ok(Some(VcpuExit::Hlt))));

        // State round trip; a foreign architecture's state is rejected.
        let state = vcpu.get_state().unwrap();
        let mut other = EbpfVcpu::new(4, memory.clone(), EbpfConfig::default());
        other.set_state(&state).unwrap();
        assert_eq!(other.cpu().regs, vcpu.cpu().regs);
        let foreign = CpuState::riscv(Default::default());
        assert!(other.set_state(&foreign).is_err());

        // A fault is an error and halts the vCPU; wake clears the halt.
        memory
            .write_slice(&encode_basic(0x61, 0, 1, 0, 0, le), GuestAddress(0))
            .unwrap();
        let mut faulting = EbpfVcpu::new(5, memory.clone(), EbpfConfig::default());
        faulting.cpu_mut().regs.r[1] = 0x10_0000;
        assert!(faulting.run().is_err());
        assert!(matches!(faulting.run(), Ok(VcpuExit::Hlt)));
        faulting.wake();
        assert!(faulting.step_insn().is_err());

        // A spin loop yields Hlt after one batch without halting.
        memory
            .write_slice(&encode_basic(0x05, 0, 0, -1, 0, le), GuestAddress(0))
            .unwrap();
        let mut spinning = EbpfVcpu::new(6, memory.clone(), EbpfConfig::default());
        assert!(matches!(spinning.run(), Ok(VcpuExit::Hlt)));
        assert_eq!(spinning.instruction_count(), 100_000);
        assert!(matches!(spinning.step_insn(), Ok(None)));
        assert_eq!(spinning.instruction_count(), 100_001);

        // Stores, atomics, and loads reach the shared guest memory:
        // *(u64 *)(r1 + 0) = r2; lock *(u64 *)(r1 + 0) += r3;
        // r0 = *(u64 *)(r1 + 0); exit
        let program = [
            encode_basic(0x7b, 1, 2, 0, 0, le),
            encode_basic(0xdb, 1, 3, 0, 0x00, le),
            encode_basic(0x79, 0, 1, 0, 0, le),
            encode_basic(0x95, 0, 0, 0, 0, le),
        ]
        .concat();
        memory.write_slice(&program, GuestAddress(0)).unwrap();
        let mut storing = EbpfVcpu::new(7, memory.clone(), EbpfConfig::default());
        storing.cpu_mut().regs.r[1] = 0x800;
        storing.cpu_mut().regs.r[2] = 0x1122_3344_5566_7788;
        storing.cpu_mut().regs.r[3] = 0x0101_0101_0101_0101;
        assert!(matches!(storing.run(), Ok(VcpuExit::Shutdown)));
        let stored: u64 = memory.read_obj(GuestAddress(0x800)).unwrap();
        assert_eq!(stored, 0x1223_3445_5667_7889);
        assert_eq!(storing.cpu().regs.r[0], stored);

        // A store outside guest memory is a write fault at its start address.
        let mut store_fault = EbpfVcpu::new(8, memory, EbpfConfig::default());
        store_fault.cpu_mut().regs.r[1] = 0x10_0000;
        let error = store_fault.run().unwrap_err().to_string();
        assert!(
            error.contains("addr: 1048576, size: 8, write: true"),
            "{error}"
        );
        assert_eq!(store_fault.current_pc(), 0);
    }

    #[test]
    fn solana_sbpf_memory_mapping_enforces_regions_and_permissions() {
        let mut mapping = SbpfMemoryMapping::new_solana_default(
            b"\x95\x00\x00\x00\x00\x00\x00\x00",
            b"readonly_rodata!",
            4096,
            4096,
            b"input_payload_00",
        );

        let ro_val = mapping
            .load(MM_RODATA_START, EbpfMemWidth::B8, false, Endian::Little)
            .unwrap();
        assert_eq!(ro_val, u64::from_le_bytes(*b"readonly"));

        // Writing to read-only rodata must fault with write = true
        let err = mapping
            .store(
                MM_RODATA_START,
                0xdead_beef,
                EbpfMemWidth::B4,
                Endian::Little,
            )
            .unwrap_err();
        assert!(err.write);
        assert_eq!(err.addr, MM_RODATA_START);

        // Writing and reading in stack and heap succeeds
        mapping
            .store(
                MM_STACK_START + 16,
                0x1122_3344_5566_7788,
                EbpfMemWidth::B8,
                Endian::Little,
            )
            .unwrap();
        let stack_val = mapping
            .load(MM_STACK_START + 16, EbpfMemWidth::B8, false, Endian::Little)
            .unwrap();
        assert_eq!(stack_val, 0x1122_3344_5566_7788);
    }
}
