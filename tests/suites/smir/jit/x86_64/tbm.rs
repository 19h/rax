//! End-to-end AMD TBM direct-vs-native-JIT differential coverage.

use super::*;

const DATA: u64 = 0x20_0000;
const SOURCE: u64 = 0xFEDC_BA98_7654_3210;
const INITIAL_FLAGS: u64 = 0xCD7;

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn seed(vcpu: &mut X86_64Vcpu) {
    vcpu.set_tbm_enabled(true);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 0x0101_0101_0101_0101;
    regs.rcx = SOURCE;
    regs.rdx = 0x0303_0303_0303_0303;
    regs.rbx = DATA;
    regs.rsp = 0x11_0000;
    regs.rbp = SOURCE;
    regs.rsi = 0x0707_0707_0707_0707;
    regs.rdi = 0x0808_0808_0808_0808;
    regs.r8 = 0x0909_0909_0909_0909;
    regs.r9 = 0x0A0A_0A0A_0A0A_0A0A;
    regs.r10 = 0x0B0B_0B0B_0B0B_0B0B;
    regs.r11 = 0x0C0C_0C0C_0C0C_0C0C;
    regs.r12 = 0x0D0D_0D0D_0D0D_0D0D;
    regs.r13 = 0x0E0E_0E0E_0E0E_0E0E;
    regs.r14 = 0x0F0F_0F0F_0F0F_0F0F;
    regs.r15 = 0x1010_1010_1010_1010;
    regs.rflags = INITIAL_FLAGS;
    vcpu.set_regs(&regs).unwrap();
}

fn xop_p1(destination: u8, width64: bool) -> u8 {
    (u8::from(width64) << 7) | (((!destination) & 0x0F) << 3)
}

fn map9_register(opcode: u8, extension: u8, destination: u8, source: u8, width64: bool) -> Vec<u8> {
    vec![
        0x8F,
        0xE9,
        xop_p1(destination, width64),
        opcode,
        0xC0 | (extension << 3) | source,
    ]
}

fn map9_memory(opcode: u8, extension: u8, destination: u8, width64: bool) -> Vec<u8> {
    vec![
        0x8F,
        0xE9,
        xop_p1(destination, width64),
        opcode,
        (extension << 3) | 3, // [rbx]
    ]
}

fn immediate_bextr_register(destination: u8, source: u8, width64: bool, control: u32) -> Vec<u8> {
    let mut bytes = vec![
        0x8F,
        0xEA,
        xop_p1(0, width64),
        0x10,
        0xC0 | (destination << 3) | source,
    ];
    bytes.extend_from_slice(&control.to_le_bytes());
    bytes
}

fn immediate_bextr_memory(destination: u8, width64: bool, control: u32) -> Vec<u8> {
    let mut bytes = vec![
        0x8F,
        0xEA,
        xop_p1(0, width64),
        0x10,
        (destination << 3) | 3, // [rbx]
    ];
    bytes.extend_from_slice(&control.to_le_bytes());
    bytes
}

fn assert_direct_jit_equivalent(name: &str, instruction: &[u8], memory_source: bool) {
    let mut code = instruction.to_vec();
    code.push(0xF4);

    let (mut direct, direct_memory) = make_vcpu_mem(&code);
    seed(&mut direct);
    direct_memory.write_obj(SOURCE, GuestAddress(DATA)).unwrap();
    assert!(
        direct
            .step()
            .unwrap_or_else(|error| panic!("{name} direct: {error:?}"))
            .is_none(),
        "{name}: direct instruction exit"
    );
    let expected = direct.get_regs().unwrap();

    let (mut jit, jit_memory) = make_vcpu_mem(&code);
    seed(&mut jit);
    jit_memory.write_obj(SOURCE, GuestAddress(DATA)).unwrap();
    jit.set_jit_call(false);
    jit.set_jit_mem(memory_source);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{name} JIT: {error:?}")),
        "{name}: TBM must enter the native tier:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    let actual = jit.get_regs().unwrap();

    assert_eq!(gprs(&actual), gprs(&expected), "{name}: GPR file");
    assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
    assert_eq!(actual.rip, expected.rip, "{name}: RIP");
    assert_eq!(
        jit_memory.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
        direct_memory.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
        "{name}: source memory"
    );
}

#[test]
fn jit_all_tbm_operations_widths_and_operand_planes_match_direct_execution() {
    let operations = [
        ("blcfill", 0x01, 1),
        ("blsfill", 0x01, 2),
        ("blcs", 0x01, 3),
        ("tzmsk", 0x01, 4),
        ("blcic", 0x01, 5),
        ("blsic", 0x01, 6),
        ("t1mskc", 0x01, 7),
        ("blcmsk", 0x02, 1),
        ("blci", 0x02, 6),
    ];

    for (mnemonic, opcode, extension) in operations {
        for width64 in [false, true] {
            let width = if width64 { 64 } else { 32 };
            assert_direct_jit_equivalent(
                &format!("{mnemonic} rax,rcx W{width}"),
                &map9_register(opcode, extension, 0, 1, width64),
                false,
            );
            assert_direct_jit_equivalent(
                &format!("{mnemonic} rax,[rbx] W{width}"),
                &map9_memory(opcode, extension, 0, width64),
                true,
            );
            assert_direct_jit_equivalent(
                &format!("{mnemonic} rsp,[rbx] W{width}"),
                &map9_memory(opcode, extension, 4, width64),
                true,
            );
            assert_direct_jit_equivalent(
                &format!("{mnemonic} rsp,rbp W{width}"),
                &map9_register(opcode, extension, 4, 5, width64),
                false,
            );
            assert_direct_jit_equivalent(
                &format!("{mnemonic} rbp,rbp W{width}"),
                &map9_register(opcode, extension, 5, 5, width64),
                false,
            );
        }
    }
}

#[test]
fn jit_immediate_bextr_controls_memory_and_stack_aliases_match_direct_execution() {
    for width64 in [false, true] {
        let width = if width64 { 64 } else { 32 };
        for control in [0, 0x0804, 0x0840, 0x4004] {
            assert_direct_jit_equivalent(
                &format!("bextr rax,rcx,{control:#06x} W{width}"),
                &immediate_bextr_register(0, 1, width64, control),
                false,
            );
            assert_direct_jit_equivalent(
                &format!("bextr rax,[rbx],{control:#06x} W{width}"),
                &immediate_bextr_memory(0, width64, control),
                true,
            );
            assert_direct_jit_equivalent(
                &format!("bextr rbp,[rbx],{control:#06x} W{width}"),
                &immediate_bextr_memory(5, width64, control),
                true,
            );
            assert_direct_jit_equivalent(
                &format!("bextr rsp,rbp,{control:#06x} W{width}"),
                &immediate_bextr_register(4, 5, width64, control),
                false,
            );
            assert_direct_jit_equivalent(
                &format!("bextr rbp,rbp,{control:#06x} W{width}"),
                &immediate_bextr_register(5, 5, width64, control),
                false,
            );
        }
    }
}

#[test]
fn jit_tbm_memory_faults_preserve_complete_restart_state() {
    for (name, instruction) in [
        ("map9 identity destination", map9_memory(0x01, 1, 0, true)),
        (
            "map9 state-backed destination",
            map9_memory(0x02, 6, 4, false),
        ),
        (
            "BEXTR identity destination",
            immediate_bextr_memory(0, true, 0x0804),
        ),
        (
            "BEXTR state-backed destination",
            immediate_bextr_memory(5, false, 0x0804),
        ),
    ] {
        let mut code = instruction;
        code.push(0xF4);
        let mut jit = make_vcpu_code(&code);
        seed(&mut jit);
        let mut before = jit.get_regs().unwrap();
        before.rbx = MEM_SIZE + 0x1000;
        jit.set_regs(&before).unwrap();
        jit.set_jit_call(false);
        jit.set_jit_mem(true);

        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name}: faulting TBM load must compile before precise deoptimization"
        );
        let after = jit.get_regs().unwrap();
        assert_eq!(gprs(&after), gprs(&before), "{name}: GPR file");
        assert_eq!(after.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(after.rip, LOAD_ADDR, "{name}: restart PC");
    }
}

#[test]
fn jit_tbm_guard_is_dynamic_precise_and_noncommitting() {
    let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12]; // mov esi,0x12345678
    code.extend_from_slice(&map9_register(0x01, 1, 0, 1, true));
    code.push(0xF4);

    for (name, enabled, protected, vm) in [
        ("feature absent", false, true, false),
        ("real mode", true, false, false),
        ("virtual-8086 mode", true, true, true),
    ] {
        let mut jit = make_vcpu_code(&code);
        seed(&mut jit);
        jit.set_jit_call(false);
        assert!(jit.jit_try_block().expect("prime guarded TBM region"));

        seed(&mut jit);
        jit.set_tbm_enabled(enabled);
        let mut regs = jit.get_regs().unwrap();
        regs.rip = LOAD_ADDR;
        regs.rsi = 0;
        if vm {
            regs.rflags |= 1 << 17;
        }
        let before = regs.clone();
        jit.set_regs(&regs).unwrap();
        let mut sregs = jit.get_sregs().unwrap();
        if protected {
            sregs.cr0 |= 1;
        } else {
            sregs.cr0 &= !1;
        }
        jit.set_sregs(&sregs).unwrap();

        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name} cached JIT: {error:?}")),
            "{name}: native prefix must run before the dynamic TBM guard"
        );
        let guarded = jit.get_regs().unwrap();
        assert_eq!(guarded.rsi, 0x1234_5678, "{name}: native prefix");
        assert_eq!(guarded.rax, before.rax, "{name}: destination commit");
        assert_eq!(guarded.rcx, before.rcx, "{name}: source commit");
        assert_eq!(guarded.rflags, before.rflags, "{name}: flag commit");
        assert_eq!(guarded.rip, LOAD_ADDR + 5, "{name}: exact frontier");

        let error = match jit.step() {
            Err(error) => format!("{error:#}"),
            Ok(exit) => panic!("{name}: direct replay unexpectedly succeeded: {exit:?}"),
        };
        assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
        let after_fault = jit.get_regs().unwrap();
        assert_eq!(after_fault.rax, before.rax, "{name}: direct destination");
        assert_eq!(after_fault.rcx, before.rcx, "{name}: direct source");
        assert_eq!(after_fault.rflags, before.rflags, "{name}: direct flags");
        assert_eq!(after_fault.rip, LOAD_ADDR + 5, "{name}: direct RIP");
    }
}

#[test]
fn jit_tbm_compatibility_mode_replays_direct_wig_semantics() {
    let mut code = map9_register(0x02, 6, 0, 3, true);
    code.push(0xF4);
    let source = 0x0123_4567_89AB_CDEF_u64;

    let mut jit = make_vcpu_code(&code);
    seed(&mut jit);
    let mut regs = jit.get_regs().unwrap();
    regs.rbx = source;
    jit.set_regs(&regs).unwrap();
    let mut sregs = jit.get_sregs().unwrap();
    sregs.cs.l = false;
    sregs.cs.db = true;
    sregs.efer |= 1 << 10;
    jit.set_sregs(&sregs).unwrap();
    jit.set_jit_call(false);

    assert!(
        jit.jit_try_block().expect("compile guarded TBM region"),
        "the compatibility-mode guard must form a precise native frontier"
    );
    let guarded = jit.get_regs().unwrap();
    assert_eq!(guarded.rip, LOAD_ADDR);
    assert_eq!(guarded.rax, regs.rax);
    assert_eq!(guarded.rbx, source);
    assert_eq!(guarded.rflags, regs.rflags);

    assert!(jit.step().expect("replay compatibility-mode TBM").is_none());
    let replayed = jit.get_regs().unwrap();
    let source32 = source as u32;
    assert_eq!(
        replayed.rax,
        u64::from(source32 | !source32.wrapping_add(1)),
        "XOP.W=1 must remain WIG outside 64-bit mode"
    );
    assert_eq!(replayed.rbx, source);
    assert_eq!(replayed.rip, LOAD_ADDR + 5);
}
