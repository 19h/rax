//! Exact helper-backed VEX.128 high/low 64-bit lane store coverage.

use super::*;
use crate::smir::ir::X86VexHalfMoveStoreEncoding;
use crate::smir::lower::X86_GUEST_VEC_STORE_FN_OFFSET;
use crate::smir::lower::runtime::{
    X86JitVexHalfMoveStoreSequence, x86_jit_vex_half_move_store_sequence,
};

#[cfg(target_arch = "x86_64")]
use crate::smir::interpret::{BlockResult, SmirInterpreter};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::flags::MaterializedFlags;
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::memory::FlatMemory;
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::memory::SmirMemory;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HalfMoveStoreCase {
    lane: MemoryLane,
    format: MoveFormat,
    form: VexForm,
    source: u8,
    base: u8,
}

impl HalfMoveStoreCase {
    const fn opcode(self) -> u8 {
        match self.lane {
            MemoryLane::Low => 0x13,
            MemoryLane::High => 0x17,
        }
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.source < 16 && self.base < 16);
        let encoded_reserved_vvvv = 0x78;
        let modrm = 0x40 | ((self.source & 7) << 3) | (self.base & 7);
        let mut bytes = match self.form {
            VexForm::C5 => {
                assert!(self.base < 8, "C5 has no VEX.B extension");
                vec![
                    0xC5,
                    (if self.source < 8 { 0x80 } else { 0 })
                        | encoded_reserved_vvvv
                        | self.format.pp(),
                    self.opcode(),
                    modrm,
                ]
            }
            VexForm::C4W0 | VexForm::C4W1 => vec![
                0xC4,
                (if self.source < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | encoded_reserved_vvvv | self.format.pp(),
                self.opcode(),
                modrm,
            ],
        };
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes
    }

    fn expected_encoding(self) -> X86VexHalfMoveStoreEncoding {
        X86VexHalfMoveStoreEncoding {
            source: self.source,
            memory_lane: self.lane.index(),
            w: self.form.w(),
            pp: self.format.pp(),
            opcode: self.opcode(),
        }
    }

    fn expected_scratch_store_bytes(self) -> Vec<u8> {
        let mut bytes = vec![
            0xC5,
            (if self.source < 8 { 0x80 } else { 0 }) | 0x78,
            self.opcode(),
            0x80 | ((self.source & 7) << 3),
        ];
        bytes.extend_from_slice(&X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes());
        bytes
    }
}

fn classified_store_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitVexHalfMoveStoreSequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_half_move_store_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified_store(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexHalfMoveStoreSequence> {
    classified_store_at(function, 0, allow_mem)
}

fn assert_exact_store_lift_and_sequence(function: &SmirFunction, case: HalfMoveStoreCase) {
    let ops = &function.blocks[0].ops;
    assert_eq!(ops.len(), 2, "{case:?}: {ops:#?}");
    let extracted = match &ops[0].kind {
        OpKind::VExtractLane {
            dst: value @ VReg::Virtual(_),
            vec,
            lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*vec, xmm(case.source), "{case:?}");
            assert_eq!(*lane, case.lane.index(), "{case:?}");
            *value
        }
        other => panic!("{case:?}: expected source-lane extraction, got {other:?}"),
    };
    assert!(matches!(
        &ops[1].kind,
        OpKind::Store {
            src,
            width: MemWidth::B8,
            ..
        } if *src == extracted
    ));
    assert!(
        ops.iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none())
    );
    assert_eq!(
        classified_store(function, true),
        Some(X86JitVexHalfMoveStoreSequence {
            consumed: 2,
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(classified_store(function, false), None, "{case:?}");
}

fn lift_store_case(case: HalfMoveStoreCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_store_lift_and_sequence(&function, case);
    function
}

fn assert_store_feature_requirements(function: &SmirFunction, case: HalfMoveStoreCase) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let mut expected = X86NativeReplayFeatureRequirements::default();
    expected.any = true;
    expected.all_spans_support_avx_ymm16 = true;
    expected.needs_avx = true;
    assert_eq!(
        x86_native_replay_feature_requirements(function, &excluded),
        expected,
        "{case:?}"
    );
}

fn lower_store(function: &SmirFunction, case: HalfMoveStoreCase) -> (Vec<u8>, usize) {
    assert_exact_store_lift_and_sequence(function, case);
    assert_store_feature_requirements(function, case);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX half-move store failed: {error:?}"));
    assert!(result.relocations.is_empty());
    let code = lowerer.finalize().expect("finalize VEX half-move store");
    let expected = case.expected_scratch_store_bytes();
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{case:?}: missing exact internal qword store {expected:02X?}"
    );
    assert!(
        code.windows(4)
            .any(|window| window == (X86_GUEST_VEC_STORE_FN_OFFSET as u32).to_le_bytes()),
        "{case:?}: vector-store helper offset absent"
    );
    (code, result.entry_offset)
}

#[test]
fn all_96_scanner_memory_destination_cells_admit_and_lower_at_o0_o1_o2() {
    let mut cells = 0usize;
    let mut lowered = 0usize;
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for form in VexForm::ALL {
                for source in 0..8 {
                    let case = HalfMoveStoreCase {
                        lane,
                        format,
                        form,
                        source,
                        base: 2,
                    };
                    cells += 1;
                    for level in LEVELS {
                        let function = optimize(lift_store_case(case), level);
                        lower_store(&function, case);
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cells, 96);
    assert_eq!(lowered, 96 * LEVELS.len());
}

#[test]
fn high_sources_and_complete_address_shapes_remain_exact() {
    let cases: &[(HalfMoveStoreCase, &[u8])] = &[
        (
            HalfMoveStoreCase {
                lane: MemoryLane::Low,
                format: MoveFormat::Ps,
                form: VexForm::C5,
                source: 1,
                base: 5,
            },
            &[0x64, 0xC5, 0xF8, 0x13, 0x4D, 0x20],
        ),
        (
            HalfMoveStoreCase {
                lane: MemoryLane::High,
                format: MoveFormat::Pd,
                form: VexForm::C4W1,
                source: 9,
                base: 12,
            },
            &[0x65, 0xC4, 0x01, 0xF9, 0x17, 0x4C, 0xEC, 0x20],
        ),
        (
            HalfMoveStoreCase {
                lane: MemoryLane::Low,
                format: MoveFormat::Pd,
                form: VexForm::C4W0,
                source: 14,
                base: 5,
            },
            &[
                0x67, 0xC4, 0x61, 0x79, 0x13, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
            ],
        ),
    ];
    for &(case, bytes) in cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            assert_exact_store_lift_and_sequence(&function, case);
            lower_store(&function, case);
        }
    }
}

fn assert_store_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_store(function, true),
        None,
        "{name}: sequence classifier admitted malformed store IR"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed store IR"
    );
}

fn baseline_store_case() -> HalfMoveStoreCase {
    HalfMoveStoreCase {
        lane: MemoryLane::High,
        format: MoveFormat::Pd,
        form: VexForm::C4W1,
        source: 9,
        base: 11,
    }
}

#[test]
fn reserved_fields_load_register_and_nonexact_source_metadata_fail_closed() {
    let case = baseline_store_case();
    let base = lift_store_case(case);
    let valid = case.bytes();
    let mut invalid = Vec::new();

    let mut reserved_vvvv = valid.clone();
    reserved_vvvv[2] &= !0x08;
    invalid.push(("reserved VEX.vvvv", reserved_vvvv));
    let mut l1 = valid.clone();
    l1[2] |= 0x04;
    invalid.push(("VEX.L=1", l1));
    for (name, opcode) in [
        ("low load", 0x12),
        ("high load", 0x16),
        ("different store", 0x11),
    ] {
        let mut bytes = valid.clone();
        bytes[3] = opcode;
        invalid.push((name, bytes));
    }
    for (name, pp) in [("F3 prefix", 2), ("F2 prefix", 3)] {
        let mut bytes = valid.clone();
        bytes[2] = (bytes[2] & !3) | pp;
        invalid.push((name, bytes));
    }
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
    invalid.push(("wrong map", wrong_map));
    let mut register = valid.clone();
    register[4] |= 0xC0;
    register.pop();
    invalid.push(("register destination", register));
    let mut trailing = valid.clone();
    trailing.push(0);
    invalid.push(("trailing byte", trailing));
    let mut forbidden_prefix = valid.clone();
    forbidden_prefix.insert(0, 0x66);
    invalid.push(("forbidden legacy prefix", forbidden_prefix));

    for (name, bytes) in invalid {
        let mut function = base.clone();
        function.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&bytes).expect("mutated source image fits metadata"),
        );
        assert_store_rejected(name, &function);
    }

    let mut missing = base;
    missing.x86_instruction_bytes.clear();
    assert_store_rejected("missing source metadata", &missing);
}

#[test]
fn classifier_rejects_every_store_graph_field_provenance_and_virtual_escape_mutation() {
    let case = baseline_store_case();
    let base = lift_store_case(case);
    let extracted = match base.blocks[0].ops[0].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    macro_rules! mutate_extract {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::VExtractLane { $field, .. } = &mut function.blocks[0].ops[0].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_extract!("extract destination", dst, x86(X86Reg::Rax));
    mutate_extract!("extract source", vec, xmm(8));
    mutate_extract!("extract lane", lane, 0);
    mutate_extract!("extract element", elem, VecElementType::I32);
    mutate_extract!("extract extension", sign, SignExtend::Sign);

    macro_rules! mutate_store {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::Store { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_store!("store source", src, x86(X86Reg::Rax));
    mutate_store!(
        "store address",
        addr,
        Address::Direct(VReg::Virtual(VirtualId(0xFF00)))
    );
    mutate_store!("store width", width, MemWidth::B4);

    for index in 0..2 {
        let mut function = base.clone();
        function.blocks[0].ops[index].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: case.opcode(),
            width: VecWidth::V128,
            w: case.form.w(),
        });
        malformed.push(("invented operation hint", function));
    }

    let mut split_pc = base.clone();
    split_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest provenance", split_pc));

    let mut escaped = base.clone();
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FE0),
        PC + 1,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(extracted),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extracted value escapes", escaped));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FE1),
        PC + 1,
        OpKind::Mov {
            dst: extracted,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extracted value redefined", duplicate_definition));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FE2), PC, OpKind::Nop));
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_store_rejected(name, &function);
    }

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7FE3), PC, OpKind::Nop));
    assert_eq!(
        classified_store_at(&same_pc_head, 1, true),
        None,
        "same-PC head must prevent mid-instruction admission"
    );
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct StoreMemoryContext {
    ok: u64,
    calls: u64,
    commits: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    observed: u64,
    committed: u64,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_store_helper(state: *mut GuestRegs, addr: u64, source: u32, size: u32) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut StoreMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = source;
    context.last_size = size;
    context.observed = state.vector_scratch[0];
    if context.ok == 0 || source != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX || size != 8 {
        return 0;
    }
    context.commits += 1;
    context.committed = context.observed;
    1
}

#[cfg(target_arch = "x86_64")]
fn full_store_guest_regs(case: HalfMoveStoreCase, seed: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1800u64
                .wrapping_add((index as u64) * 0x111)
                .wrapping_add((seed as u64) * 0x20)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x195)) & 0x8D5),
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: std::array::from_fn(|index| 0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 9) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((seed as u32) & 0x3F),
        vector_scratch: std::array::from_fn(|index| {
            0xCCDD_EEFF_0011_2233u64 ^ (index as u64).wrapping_mul(0x1111_1111_1111_1111)
        }),
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((index * 13 + word * 7) as u32)
                ^ (index as u64).wrapping_mul(0x8040_2010_0804_0201)
                ^ (seed as u64).wrapping_mul(0x1020_4081_0204_0810)
                ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
        });
    }
    registers.gpr[usize::from(case.base)] = 0x3000 + ((seed & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn store_effective_address(registers: &GuestRegs, case: HalfMoveStoreCase) -> u64 {
    registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64)
}

#[cfg(target_arch = "x86_64")]
fn interpreted_store_value(
    function: &SmirFunction,
    initial: &GuestRegs,
    address: u64,
    case: HalfMoveStoreCase,
) -> u64 {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let sentinel = [0xA5; 24];
    memory.load(address as usize - 8, &sentinel);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr, initial.gpr, "{case:?}: GPRs");
    for (index, value) in initial.zmm.iter().enumerate() {
        assert_eq!(&x86.xmm[index][..8], value, "{case:?}: ZMM{index}");
    }
    assert_eq!(x86.k, initial.k, "{case:?}: masks");
    assert_eq!(x86.rflags, initial.rflags, "{case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, initial.mxcsr, "{case:?}: MXCSR");

    let mut actual = [0u8; 24];
    memory.read(address - 8, &mut actual).unwrap();
    assert_eq!(actual[..8], sentinel[..8], "{case:?}: leading bytes");
    assert_eq!(actual[16..], sentinel[16..], "{case:?}: trailing bytes");
    u64::from_le_bytes(actual[8..16].try_into().unwrap())
}

#[cfg(target_arch = "x86_64")]
fn assert_store_helper_observation(
    context: &StoreMemoryContext,
    address: u64,
    expected: u64,
    commits: u64,
    case: HalfMoveStoreCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(context.last_size, 8, "{label} {case:?}");
    assert_eq!(context.observed, expected, "{label} {case:?}");
    assert_eq!(context.commits, commits, "{label} {case:?}");
    if commits != 0 {
        assert_eq!(context.committed, expected, "{label} {case:?}");
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeStoreCase {
    level: OptLevel,
    instruction: HalfMoveStoreCase,
    seed: usize,
}

#[cfg(target_arch = "x86_64")]
fn native_store_cases() -> Vec<NativeStoreCase> {
    let c5_bases = [0u8, 2, 4, 5, 7, 1, 3, 6];
    let c4_bases = [11u8, 12, 4, 5, 14, 0, 2, 15];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [OptLevel::O0, OptLevel::O2] {
        for lane in MemoryLane::ALL {
            for format in MoveFormat::ALL {
                for form in VexForm::ALL {
                    for source in 0..16u8 {
                        let base = if form == VexForm::C5 {
                            c5_bases[usize::from(source) & 7]
                        } else {
                            c4_bases[usize::from(source) & 7]
                        };
                        cases.push(NativeStoreCase {
                            level,
                            instruction: HalfMoveStoreCase {
                                lane,
                                format,
                                form,
                                source,
                                base,
                            },
                            seed: ordinal,
                        });
                        ordinal += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 2 * 192);
    assert!(cases.iter().any(|case| case.instruction.source >= 8));
    assert!(cases.iter().any(|case| case.instruction.base == 4));
    assert!(cases.iter().any(|case| case.instruction.base == 5));
    cases
}

#[cfg(target_arch = "x86_64")]
const STORE_CHILD_RANGE_ENV: &str = "RAX_VEX_HALF_MOVE_STORE_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn store_child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(STORE_CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {STORE_CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {STORE_CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {STORE_CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn execute_native_store_case_range(cases: &[NativeStoreCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    let executions = range.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for native_case in &cases[range] {
        let case = native_case.instruction;
        let function = optimize(lift_store_case(case), native_case.level);
        let (code, entry) = lower_store(&function, case);
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{:?} {case:?}: {error:?}", native_case.level));

        let mut context = StoreMemoryContext {
            ok: 1,
            calls: 0,
            commits: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            observed: 0,
            committed: 0,
        };
        let mut registers = full_store_guest_regs(case, native_case.seed);
        let address = store_effective_address(&registers, case);
        registers.ctx = (&mut context as *mut StoreMemoryContext) as u64;
        registers.vec_store_fn = vector_store_helper as usize as u64;
        let initial = registers;
        let stored = interpreted_store_value(&function, &initial, address, case);
        let mut expected = initial;
        expected.vector_scratch[0] = stored;

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: success",
            native_case.level
        );
        assert_store_helper_observation(&context, address, stored, 1, case, "success");
        successes += 1;

        let mut context = StoreMemoryContext {
            ok: 0,
            calls: 0,
            commits: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            observed: 0,
            committed: 0,
        };
        let mut registers = full_store_guest_regs(case, native_case.seed ^ 0x55);
        let address = store_effective_address(&registers, case);
        registers.ctx = (&mut context as *mut StoreMemoryContext) as u64;
        registers.vec_store_fn = vector_store_helper as usize as u64;
        let initial = registers;
        let stored = interpreted_store_value(&function, &initial, address, case);
        let mut expected = initial;
        expected.vector_scratch[0] = stored;
        expected.exit_pc = PC;

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: fault",
            native_case.level
        );
        assert_store_helper_observation(&context, address, stored, 0, case, "fault");
        faults += 1;
    }
    assert_eq!(successes, executions);
    assert_eq!(faults, executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX half-move store cases"
    );
}

#[cfg(target_arch = "x86_64")]
fn run_store_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(
            STORE_CHILD_RANGE_ENV,
            format!("{}:{}", range.start, range.end),
        )
        .output()
        .expect("run isolated native VEX half-move store differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_store_differential(test_name: &str) {
    let cases = native_store_cases();
    if let Some(range) = store_child_range() {
        execute_native_store_case_range(&cases, range);
        return;
    }

    let whole = run_store_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }

    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_store_child_range(test_name, start..middle)
            .status
            .success()
        {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_store_child_range(test_name, start..end);
    let case = cases[start];
    let bytes = case.instruction.bytes();
    panic!(
        "isolated native VEX half-move store failure at case {start}/{}: \
         {case:?} {bytes:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_stores_match_o0_o2_interpreter_and_fault_without_memory_commit() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX half-move store differential: host lacks AVX");
        return;
    }
    run_isolated_native_store_differential(
        "smir::lower::runtime::jit_gate_tests::vex_half_move_memory_source::store::\
         native_stores_match_o0_o2_interpreter_and_fault_without_memory_commit",
    );
}
