//! RFC 9669 platform facilities on every execution plane.
//!
//! RFC 9669 places LDDW subtypes 0x1-0x6 in the base64 conformance group
//! (section 5.4, Table 12) and the helper call by BTF ID in base32 (section
//! 4.3.1). Their operands are platform objects: maps, platform variables, and
//! helpers. These tests bind those objects and require every plane to compute
//! the value Table 12 defines, to trap precisely when an object is unbound,
//! and to exit to the host for a BTF helper in its own key namespace. The
//! expected values below are computed from the RFC formulas, not from the
//! engine.

use super::*;
use rax::isa::ebpf::{EbpfHelperKey, EbpfMap, EbpfPlatform};

/// Guest byte address of the LDDW under test.
const LDDW_PC: u64 = 16;
/// Initial value of the LDDW destination `r3`.
const SENTINEL: u64 = 0x5a5a_5a5a_5a5a_5a5a;

const FD_MAP_ADDRESS: u64 = 0xffff_8000_0000_1000;
const FD_MAP_VALUE: u64 = 0x2000;
const SPARSE_MAP_ADDRESS: u64 = 0xabc0;
const INDEX_MAP_ADDRESS: u64 = 0x7000;
const INDEX_MAP_VALUE: u64 = 0x9000;
const VARIABLE_ADDRESS: u64 = 0x5555_0000;

fn platform() -> EbpfPlatform {
    let mut platform = EbpfPlatform::new();
    platform.maps_by_fd.insert(
        3,
        EbpfMap {
            address: FD_MAP_ADDRESS,
            value_address: Some(FD_MAP_VALUE),
        },
    );
    // A map without a single contiguous region has no `map_val`.
    platform.maps_by_fd.insert(
        4,
        EbpfMap {
            address: SPARSE_MAP_ADDRESS,
            value_address: None,
        },
    );
    platform.maps_by_idx.insert(
        0,
        EbpfMap {
            address: INDEX_MAP_ADDRESS,
            value_address: Some(INDEX_MAP_VALUE),
        },
    );
    platform.variables.insert(11, VARIABLE_ADDRESS);
    platform
}

/// `(src_type, imm, next_imm, expected)` with the Table 12 result, or `None`
/// for an unbound object. `next_imm` is a signed offset (section 3.2), and
/// `code_addr(imm)` is the instruction `imm + 1` slots after the LDDW's
/// first slot (the Linux verifier's `i + imm + 1`).
fn cases() -> Vec<(u8, i32, i32, Option<u64>)> {
    let code = |imm: i32| LDDW_PC.wrapping_add((i64::from(imm) + 1).wrapping_mul(8) as u64);
    let offset = |base: u64, next_imm: i32| base.wrapping_add(i64::from(next_imm) as u64);
    vec![
        (
            0,
            0x1234_5678,
            0x9abc_def0_u32 as i32,
            Some(0x9abc_def0_1234_5678),
        ),
        (0, -1, -1, Some(u64::MAX)),
        (1, 3, 0, Some(FD_MAP_ADDRESS)),
        (1, 4, 0, Some(SPARSE_MAP_ADDRESS)),
        (1, 5, 0, None),
        (2, 3, 0, Some(FD_MAP_VALUE)),
        (2, 3, 16, Some(FD_MAP_VALUE + 16)),
        (2, 3, -8, Some(FD_MAP_VALUE - 8)),
        (2, 3, i32::MIN, Some(offset(FD_MAP_VALUE, i32::MIN))),
        (2, 3, i32::MAX, Some(offset(FD_MAP_VALUE, i32::MAX))),
        (2, 4, 0, None),
        (2, 5, 0, None),
        (3, 11, 0, Some(VARIABLE_ADDRESS)),
        (3, 12, 0, None),
        (4, 0, 0, Some(code(0))),
        (4, 2, 0, Some(code(2))),
        (4, -1, 0, Some(LDDW_PC)),
        (4, -3, 0, Some(code(-3))),
        (4, i32::MAX, 0, Some(code(i32::MAX))),
        (4, i32::MIN, 0, Some(code(i32::MIN))),
        (5, 0, 0, Some(INDEX_MAP_ADDRESS)),
        (5, 1, 0, None),
        (6, 0, 0, Some(INDEX_MAP_VALUE)),
        (6, 0, 4, Some(INDEX_MAP_VALUE + 4)),
        (6, 0, -4, Some(INDEX_MAP_VALUE - 4)),
        (6, 1, 0, None),
    ]
}

/// `mov r0, 0; ja +0; lddw r3, <subtype, imm, next_imm>; mov r0, r3; exit`
fn program(src_type: u8, imm: i32, next_imm: i32, endian: Endian) -> Vec<u8> {
    [
        encode_insn(0xb7, 0, 0, 0, 0, endian),
        encode_insn(0x05, 0, 0, 0, 0, endian),
        encode_insn(0x18, 3, src_type, 0, imm, endian),
        encode_insn(0x00, 0, 0, 0, next_imm, endian),
        encode_insn(0xbf, 0, 3, 0, 0, endian),
        encode_insn(0x95, 0, 0, 0, 0, endian),
    ]
    .concat()
}

fn config(endian: Endian) -> EbpfConfig {
    EbpfConfig {
        endian,
        platform: platform(),
        ..EbpfConfig::default()
    }
}

/// Final exit, `r3`, and instruction count on the direct CPU.
fn direct(program: &[u8], config: &EbpfConfig) -> (Result<u64, EbpfTrap>, u64, u64) {
    let mut memory = FlatMemory::new(64);
    memory.load(0, program);
    let mut cpu = EbpfCpu::new(config.clone());
    cpu.regs.r[3] = SENTINEL;
    let exit = match cpu.run(&mut memory) {
        EbpfExit::Exit { r0 } => Ok(r0),
        EbpfExit::Trap(trap) => Err(trap),
        other => panic!("unexpected exit {other:?}"),
    };
    (exit, cpu.regs.r[3], cpu.insn_count)
}

fn lift(program: &[u8], config: &EbpfConfig, opt: OptLevel) -> SmirInterpreter {
    let mut lifter = EbpfLifter::new(config.dialect, config.endian);
    lifter.platform = config.platform.clone();
    let mut lift_context = LiftContext::new(lifter.source_arch());
    lift_context.endian = config.endian;
    let mut interpreter = SmirInterpreter::new();
    for mut function in lifter
        .lift_program(0, &CodeSlice(program), &mut lift_context)
        .unwrap()
    {
        optimize_function(&mut function, opt);
        interpreter.add_function(function);
    }
    interpreter
}

/// Final exit, `r3`, and instruction count on SMIR at `opt`.
fn smir(program: &[u8], config: &EbpfConfig, opt: OptLevel) -> (Result<u64, EbpfTrap>, u64, u64) {
    let mut interpreter = lift(program, config, opt);
    let mut memory = FlatMemory::new(64);
    memory.load(0, program);
    let mut context = SmirContext::new_ebpf_with_config(config.clone());
    context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(3)), SENTINEL);
    let exit = match interpreter.run(&mut context, &mut memory) {
        ExitReason::Return { to } => Ok(to),
        ExitReason::EbpfTrap(trap) => Err(trap),
        other => panic!("unexpected exit {other:?}"),
    };
    let r3 = context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(3)));
    (exit, r3, context.insn_count)
}

#[test]
fn every_lddw_subtype_loads_its_table_12_value_on_every_plane() {
    for endian in [Endian::Little, Endian::Big] {
        let config = config(endian);
        for (src_type, imm, next_imm, expected) in cases() {
            let program = program(src_type, imm, next_imm, endian);
            let want = match expected {
                Some(value) => (Ok(value), value, 5),
                None => (
                    Err(EbpfTrap::UnsupportedPseudoLoad {
                        pc: LDDW_PC,
                        src_type,
                    }),
                    SENTINEL,
                    3,
                ),
            };
            let case = format!("{endian:?} subtype={src_type} imm={imm} next_imm={next_imm}");
            assert_eq!(direct(&program, &config), want, "direct {case}");
            for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_eq!(smir(&program, &config, opt), want, "{opt:?} {case}");
            }
        }
    }
}

#[test]
fn solana_sbpf_ignores_the_lddw_subtype_field() {
    // SBPF decodes LDDW by its immediates alone; platform bindings and the
    // subtype field have no effect (Anza `ebpf::LD_DW_IMM`).
    let mut config = EbpfConfig::solana(SbpfVersion::V0);
    config.platform = platform();
    for src_type in 0..=6 {
        let program = program(src_type, 3, 7, Endian::Little);
        let value = (7 << 32) | 3;
        let want = (Ok(value), value, 5);
        assert_eq!(direct(&program, &config), want, "direct subtype={src_type}");
        for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            assert_eq!(
                smir(&program, &config, opt),
                want,
                "{opt:?} subtype={src_type}"
            );
        }
    }
}

/// `mov r1, 5; call <kind> 77; add r0, 1; exit`
fn helper_program(src_kind: u8, endian: Endian) -> Vec<u8> {
    [
        encode_insn(0xb7, 1, 0, 0, 5, endian),
        encode_insn(0x85, 0, src_kind, 0, 77, endian),
        encode_insn(0x07, 0, 0, 0, 1, endian),
        encode_insn(0x95, 0, 0, 0, 0, endian),
    ]
    .concat()
}

#[test]
fn btf_helper_calls_exit_and_resume_identically_on_every_plane() {
    // The host implements helper 77 as `r0 = 100 + r1`.
    for endian in [Endian::Little, Endian::Big] {
        for (src_kind, key) in [(0, EbpfHelperKey::Static(77)), (2, EbpfHelperKey::Btf(77))] {
            let program = helper_program(src_kind, endian);
            let config = EbpfConfig {
                endian,
                ..EbpfConfig::default()
            };

            let mut memory = FlatMemory::new(64);
            memory.load(0, &program);
            let mut cpu = EbpfCpu::new(config.clone());
            let first = cpu.run(&mut memory);
            match key {
                EbpfHelperKey::Static(id) => {
                    assert_eq!(
                        first,
                        EbpfExit::Syscall {
                            key: id,
                            next_pc: 16
                        }
                    )
                }
                EbpfHelperKey::Btf(id) => {
                    assert_eq!(first, EbpfExit::BtfHelper { id, next_pc: 16 })
                }
            }
            assert_eq!((cpu.regs.pc, cpu.insn_count), (16, 2));
            cpu.regs.r[0] = 100 + cpu.regs.r[1];
            assert_eq!(cpu.run(&mut memory), EbpfExit::Exit { r0: 106 });
            let direct_count = cpu.insn_count;

            for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let mut interpreter = lift(&program, &config, opt);
                let mut memory = FlatMemory::new(64);
                memory.load(0, &program);
                let mut context = SmirContext::new_ebpf_with_config(config.clone());
                let ExitReason::Syscall { num, args } = interpreter.run(&mut context, &mut memory)
                else {
                    panic!("{opt:?} {key:?}: no helper exit");
                };
                assert_eq!(EbpfHelperKey::from_smir(num), key, "{opt:?}");
                assert_eq!((context.pc, context.insn_count), (16, 2), "{opt:?} {key:?}");
                context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(0)), 100 + args[0]);
                let resumed = interpreter.run(&mut context, &mut memory);
                assert!(
                    matches!(resumed, ExitReason::Return { to: 106 }),
                    "{resumed:?}"
                );
                assert_eq!(context.insn_count, direct_count, "{opt:?} {key:?}");
            }
        }
    }
}

#[test]
fn registered_btf_helpers_run_in_their_own_namespace() {
    for endian in [Endian::Little, Endian::Big] {
        let program = helper_program(2, endian);
        let mut memory = FlatMemory::new(64);
        memory.load(0, &program);
        let mut cpu = EbpfCpu::new(EbpfConfig {
            endian,
            ..EbpfConfig::default()
        });
        // A static helper with the same ID must not satisfy the BTF call.
        cpu.register_syscall(77, |_, _| Ok(999));
        cpu.register_btf_helper(77, |registers, _| Ok(100 + registers[1]));
        assert_eq!(cpu.run(&mut memory), EbpfExit::Exit { r0: 106 });
        assert_eq!(cpu.insn_count, 4);
    }
}
