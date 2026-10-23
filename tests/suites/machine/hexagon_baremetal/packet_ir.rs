//! Packet-boundary and all-or-nothing backend publication contracts.
use super::packet_memory::{context, function, machine};
use rax::config::Endianness;
use rax::smir::ir::memory::{MemoryError, PacketCommitError, PacketWrite, SmirMemory};
use rax::smir::ir::types::*;
use rax::smir::ir::{SmirBlock, Terminator, TrapKind};
use rax::smir::optimize::OptLevel;
use rax::smir::{
    BlockResult, ExitReason, FlatMemory, OpKind, SmirContext, SmirInterpreter, SmirOp,
};

struct NoPacketMemory {
    memory: FlatMemory,
    writes: usize,
}

impl SmirMemory for NoPacketMemory {
    fn read(&mut self, addr: GuestAddr, data: &mut [u8]) -> Result<(), MemoryError> {
        self.memory.read(addr, data)
    }
    fn write(&mut self, addr: GuestAddr, data: &[u8]) -> Result<(), MemoryError> {
        self.writes += 1;
        self.memory.write(addr, data)
    }
    fn probe(&self, addr: GuestAddr, size: usize, write: bool) -> Result<(), MemoryError> {
        self.memory.probe(addr, size, write)
    }
    fn atomic_load(
        &mut self,
        addr: GuestAddr,
        size: MemWidth,
        order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        self.memory.atomic_load(addr, size, order)
    }
    fn atomic_store(
        &mut self,
        addr: GuestAddr,
        value: u64,
        size: MemWidth,
        order: MemoryOrder,
    ) -> Result<(), MemoryError> {
        self.memory.atomic_store(addr, value, size, order)
    }
    fn compare_and_swap(
        &mut self,
        addr: GuestAddr,
        expected: u64,
        new: u64,
        size: MemWidth,
        success: MemoryOrder,
        failure: MemoryOrder,
    ) -> Result<(u64, bool), MemoryError> {
        self.memory
            .compare_and_swap(addr, expected, new, size, success, failure)
    }
    fn atomic_rmw(
        &mut self,
        addr: GuestAddr,
        op: AtomicOp,
        value: u64,
        size: MemWidth,
        order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        self.memory.atomic_rmw(addr, op, value, size, order)
    }
    fn load_exclusive(&mut self, addr: GuestAddr, size: MemWidth) -> Result<u64, MemoryError> {
        self.memory.load_exclusive(addr, size)
    }
    fn store_exclusive(
        &mut self,
        addr: GuestAddr,
        value: u64,
        size: MemWidth,
    ) -> Result<bool, MemoryError> {
        self.memory.store_exclusive(addr, value, size)
    }
    fn clear_exclusive(&mut self) {
        self.memory.clear_exclusive();
    }
    fn fence(&mut self, kind: FenceKind) {
        self.memory.fence(kind);
    }
}

#[test]
fn a_backend_without_packet_publication_reports_unsupported_before_any_write() {
    let (_, _, mut before) = machine(0x78004038, 0x2000, 0x1000, Endianness::Little);
    before.r[24] = 0x99;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let function = function(&[0x78004038, 0xa181c200], level);
        let mut state = context(&before);
        let mut memory = NoPacketMemory {
            memory: FlatMemory::new(0x4000),
            writes: 0,
        };
        memory.memory.write(0x2000, &[0xa5; 32]).unwrap();
        let result = SmirInterpreter::new().execute_block(
            &mut state,
            &mut memory,
            function.entry_block().unwrap(),
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::Unsupported { addr: 0x1000, .. })
            ),
            "{result:?}"
        );
        assert_eq!(memory.writes, 0);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))),
            0x99
        );
        assert_eq!(state.insn_count, 0);
        let mut bytes = [0; 32];
        memory.memory.read(0x2000, &mut bytes).unwrap();
        assert_eq!(bytes, [0xa5; 32]);
    }
}

#[test]
fn read_only_packets_do_not_require_a_batch_write_backend() {
    let (_, _, before) = machine(0x9181c018, 0x2000, 0x1000, Endianness::Little);
    let function = function(&[0x9181c018], OptLevel::O2);
    let mut state = context(&before);
    let mut memory = NoPacketMemory {
        memory: FlatMemory::new(0x4000),
        writes: 0,
    };
    memory
        .memory
        .write(0x2000, &0x88776655_u32.to_le_bytes())
        .unwrap();
    let result = SmirInterpreter::new().execute_block(
        &mut state,
        &mut memory,
        function.entry_block().unwrap(),
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(
        state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))),
        0x88776655
    );
    assert_eq!(memory.writes, 0);
    assert_eq!(state.insn_count, 1);
}

#[test]
fn flat_batch_failure_preserves_bytes_and_an_existing_reservation() {
    let mut memory = FlatMemory::new(16);
    memory.write(0, &[0xa5; 16]).unwrap();
    memory.load_exclusive(0, MemWidth::B4).unwrap();
    let result = memory.write_packet(&[
        PacketWrite {
            addr: 0,
            data: vec![1; 4],
        },
        PacketWrite {
            addr: 16,
            data: vec![2; 4],
        },
    ]);
    assert!(matches!(result, Err(PacketCommitError::Memory(_))));
    let mut bytes = [0; 16];
    memory.read(0, &mut bytes).unwrap();
    assert_eq!(bytes, [0xa5; 16]);
    assert!(memory.store_exclusive(0, 0x11223344, MemWidth::B4).unwrap());
}

#[test]
fn flat_batch_keeps_program_order_for_overlapping_writes() {
    let mut memory = FlatMemory::new(8);
    memory
        .write_packet(&[
            PacketWrite {
                addr: 0,
                data: vec![1, 2, 3, 4],
            },
            PacketWrite {
                addr: 2,
                data: vec![5, 6, 7, 8],
            },
        ])
        .unwrap();
    let mut bytes = [0; 8];
    memory.read(0, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 2, 5, 6, 7, 8, 0, 0]);
}

fn block(operations: Vec<OpKind>) -> SmirBlock {
    SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: operations
            .into_iter()
            .enumerate()
            .map(|(index, kind)| SmirOp::new(OpId(index as u16), 0x1000, kind))
            .collect(),
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    }
}

#[test]
fn malformed_packet_boundaries_reject_before_any_operation() {
    for operations in [
        vec![OpKind::HexagonPacketBegin],
        vec![OpKind::HexagonPacketCommit],
        vec![
            OpKind::HexagonPacketBegin,
            OpKind::HexagonPacketBegin,
            OpKind::HexagonPacketCommit,
        ],
    ] {
        let mut state = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(16);
        let result =
            SmirInterpreter::new().execute_block(&mut state, &mut memory, &block(operations));
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Unsupported { .. })
        ));
        assert_eq!(state.pc, 0);
        assert_eq!(state.insn_count, 0);
    }
}

#[test]
fn packet_boundaries_are_observable_optimization_and_native_admission_barriers() {
    for boundary in [OpKind::HexagonPacketBegin, OpKind::HexagonPacketCommit] {
        assert!(boundary.has_side_effects());
        assert_eq!(
            boundary.writes_memory(),
            matches!(boundary, OpKind::HexagonPacketCommit)
        );
        assert!(!boundary.is_jit_safe());
        let sources = boundary.source_vregs();
        for register in [
            HexagonReg::R(24),
            HexagonReg::P(0),
            HexagonReg::Usr,
            HexagonReg::Gp,
            HexagonReg::V(31),
            HexagonReg::Q(3),
        ] {
            assert!(sources.contains(&VReg::Arch(ArchReg::Hexagon(register))));
        }
    }
}

#[test]
fn a_packet_budget_resumes_at_a_cached_packet_boundary() {
    let (_, _, before) = machine(0x7800c038, 0x2000, 0x1000, Endianness::Little);
    let function = function(&[0x7800c038, 0x7800c058], OptLevel::O2);
    let mut state = context(&before);
    let mut memory = FlatMemory::new(0x4000);
    let mut interpreter = SmirInterpreter::new();
    interpreter.add_block(0x1000, function.entry_block().unwrap().clone());
    interpreter.set_max_insns(1);
    assert!(matches!(
        interpreter.run(&mut state, &mut memory),
        ExitReason::InsnLimit
    ));
    assert_eq!(state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))), 1);
    assert_eq!(state.pc, 0x1004);
    assert_eq!(state.insn_count, 1);
    assert!(matches!(
        interpreter.run(&mut state, &mut memory),
        ExitReason::Halt
    ));
    assert_eq!(state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))), 2);
    assert_eq!(state.pc, 0x1008);
    assert_eq!(state.insn_count, 2);
}

#[test]
fn debugger_single_step_stops_after_one_complete_packet() {
    let (_, _, before) = machine(0x7800c038, 0x2000, 0x1000, Endianness::Little);
    let function = function(&[0x7800c038, 0x7800c058], OptLevel::O2);
    let mut state = context(&before);
    state.debug.single_step = true;
    let mut memory = FlatMemory::new(0x4000);
    let mut interpreter = SmirInterpreter::new();
    interpreter.add_block(0x1000, function.entry_block().unwrap().clone());
    assert!(matches!(
        interpreter.run(&mut state, &mut memory),
        ExitReason::SingleStep
    ));
    assert_eq!(state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))), 1);
    assert_eq!(state.pc, 0x1004);
    assert_eq!(state.insn_count, 1);
}

#[path = "../../../support/hexagon.rs"]
mod lifter_stack;

#[test]
fn hardware_loop_packets_reject_until_their_commit_control_flow_is_represented() {
    use rax::smir::{HexagonLifter, LiftContext, SmirLifter};
    lifter_stack::run_on_large_lifter_stack("Hexagon loop packet admission", || {
        let mut lifter = HexagonLifter::default_isa();
        let mut lift = LiftContext::new(SourceArch::Hexagon);
        let error = lifter
            .lift_insn(0x1000, &0x7f008000_u32.to_le_bytes(), &mut lift)
            .unwrap_err();
        assert!(matches!(
            error,
            rax::smir::lift::LiftError::Unsupported { .. }
        ));
    });
}
