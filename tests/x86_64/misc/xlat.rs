// Module path for tests run via x86_64.rs
#[path = "../common/mod.rs"]
mod common;

use common::*;
use rax::cpu::Registers;

// XLAT/XLATB - Table Look-up Translation
// Locates a byte entry in a table in memory, using the contents of the AL register
// as a table index, then copies the contents of the table entry back into the AL register.
//
// The index in AL is treated as an unsigned integer.
// The base address of the table is taken from:
// - DS:BX (16-bit address size)
// - DS:EBX (32-bit address size)
// - RBX (64-bit address size)
//
// Opcode: D7
// Operation: AL := [RBX + ZeroExtend(AL)]
//
// Flags: None affected

// Table address for tests
const TABLE_ADDR: u64 = 0x3000;

#[test]
fn test_xlat_basic() {
    // Basic XLAT with simple table lookup
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0; // AL = 0, look up table[0]
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Set up translation table
    write_mem_at_u8(&mem, TABLE_ADDR, 0x42); // table[0] = 0x42

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x42, "AL should be loaded with table[0]");
}

#[test]
fn test_xlat_index_1() {
    // XLAT with index 1
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 1; // AL = 1, look up table[1]
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Set up translation table
    write_mem_at_u8(&mem, TABLE_ADDR, 0x11);     // table[0]
    write_mem_at_u8(&mem, TABLE_ADDR + 1, 0x22); // table[1]

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x22, "AL should be loaded with table[1]");
}

#[test]
fn test_xlat_index_255() {
    // XLAT with maximum index (255)
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFF; // AL = 255, look up table[255]
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Set up translation table entry at offset 255
    write_mem_at_u8(&mem, TABLE_ADDR + 255, 0xAB);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0xAB, "AL should be loaded with table[255]");
}

#[test]
fn test_xlat_ascii_to_hex_table() {
    // XLAT for ASCII to hex digit conversion (common use case)
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = b'5' as u64; // AL = ASCII '5' (0x35 = 53)
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Set up ASCII-to-hex conversion table
    // For simplicity, just set up the '5' entry
    write_mem_at_u8(&mem, TABLE_ADDR + (b'5' as u64), 0x05); // '5' -> 0x05

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x05, "AL should contain hex value 0x05");
}

#[test]
fn test_xlat_case_conversion_table() {
    // XLAT for character case conversion
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = b'a' as u64; // AL = ASCII 'a' (0x61 = 97)
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Set up lowercase-to-uppercase conversion table
    write_mem_at_u8(&mem, TABLE_ADDR + (b'a' as u64), b'A'); // 'a' -> 'A'

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, b'A' as u64, "AL should contain 'A'");
}

#[test]
fn test_xlat_preserves_upper_bits_rax() {
    // XLAT should only modify AL, not upper bits of RAX
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCD00; // AL = 0, but upper bits set
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR, 0x42);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x42, "AL should be 0x42");
    assert_eq!(final_regs.rax >> 8, 0x123456789ABCD, "Upper bits of RAX preserved");
}

#[test]
fn test_xlat_preserves_rbx() {
    // XLAT should not modify RBX
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR, 0x42);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rbx, TABLE_ADDR, "RBX unchanged");
}

#[test]
fn test_xlat_preserves_other_registers() {
    // XLAT should not affect other registers
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0;
    regs.rbx = TABLE_ADDR;
    regs.rcx = 0x1111111111111111;
    regs.rdx = 0x2222222222222222;
    regs.rsi = 0x3333333333333333;
    regs.rdi = 0x4444444444444444;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR, 0x42);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rcx, 0x1111111111111111, "RCX unchanged");
    assert_eq!(final_regs.rdx, 0x2222222222222222, "RDX unchanged");
    assert_eq!(final_regs.rsi, 0x3333333333333333, "RSI unchanged");
    assert_eq!(final_regs.rdi, 0x4444444444444444, "RDI unchanged");
}

#[test]
fn test_xlat_does_not_affect_flags() {
    // XLAT should not modify any flags
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0;
    regs.rbx = TABLE_ADDR;
    regs.rflags = 0x2 | 0x1 | (1 << 6) | (1 << 7); // CF, ZF, SF set
    let initial_flags = regs.rflags;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR, 0x42);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rflags, initial_flags, "Flags unchanged");
}

#[test]
fn test_xlat_with_different_base_addresses() {
    // XLAT with different RBX base addresses
    let base1 = 0x3000u64;
    let base2 = 0x4000u64;

    // First lookup
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 5;
    regs.rbx = base1;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, base1 + 5, 0xAA);
    write_mem_at_u8(&mem, base2 + 5, 0xBB);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0xAA, "AL should be from base1 table");
}

#[test]
fn test_xlat_sequential_lookups() {
    // Multiple XLAT lookups in sequence
    let code = [
        0xb0, 0x00, // MOV AL, 0
        0xd7,       // XLAT (lookup table[0])
        0xd7,       // XLAT again (use result as new index)
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Set up chained lookup table
    write_mem_at_u8(&mem, TABLE_ADDR + 0, 0x05); // table[0] = 5
    write_mem_at_u8(&mem, TABLE_ADDR + 5, 0x42); // table[5] = 0x42

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x42, "AL should be result of chained lookup");
}

#[test]
fn test_xlat_identity_table() {
    // XLAT with identity table (table[i] = i)
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 42;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Set up identity mapping
    write_mem_at_u8(&mem, TABLE_ADDR + 42, 42);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 42, "AL should remain 42");
}

#[test]
fn test_xlat_all_zero_table() {
    // XLAT with all-zero table
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 100;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Table defaults to 0, but explicitly set it
    write_mem_at_u8(&mem, TABLE_ADDR + 100, 0);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0, "AL should be 0");
}

#[test]
fn test_xlat_all_ones_table() {
    // XLAT with all-ones value in table
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 50;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 50, 0xFF);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0xFF, "AL should be 0xFF");
}

#[test]
fn test_xlat_with_16_entry_table() {
    // XLAT with 16-entry table (hex digit conversion)
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x0A; // AL = 10
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Hex digit to ASCII table
    write_mem_at_u8(&mem, TABLE_ADDR + 0x0A, b'A');

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, b'A' as u64, "AL should be 'A'");
}

#[test]
fn test_xlat_with_256_entry_table() {
    // XLAT with full 256-entry table
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 128;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Set up a value at index 128
    write_mem_at_u8(&mem, TABLE_ADDR + 128, 0xCC);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0xCC, "AL should be 0xCC");
}

#[test]
fn test_xlat_after_arithmetic() {
    // XLAT after arithmetic that sets AL
    let code = [
        0xb0, 0x05, // MOV AL, 5
        0x04, 0x03, // ADD AL, 3 (AL = 8)
        0xd7,       // XLAT (lookup table[8])
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 8, 0x99);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x99, "AL should be from table[8]");
}

#[test]
fn test_xlat_overwrite_previous_al() {
    // XLAT overwrites previous AL value
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xDEADBEEFCAFEBA55; // AL = 0x55
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 0x55, 0x33);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x33, "AL should be overwritten with 0x33");
}

#[test]
fn test_xlat_boundary_index_0() {
    // XLAT with boundary index 0
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR, 0x11);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x11, "AL should be table[0]");
}

#[test]
fn test_xlat_boundary_index_255() {
    // XLAT with boundary index 255
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 255;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 255, 0xEE);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0xEE, "AL should be table[255]");
}

#[test]
fn test_xlat_with_mid_range_indices() {
    // XLAT with various mid-range indices
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 64; // AL = 64
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 64, 0x40);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x40, "AL should be 0x40");
}

#[test]
fn test_xlat_lookup_sequence() {
    // Sequence of XLAT operations with different indices
    let code = [
        0xb0, 0x01, // MOV AL, 1
        0xd7,       // XLAT (table[1])
        0xb0, 0x02, // MOV AL, 2
        0xd7,       // XLAT (table[2])
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 1, 0xAA);
    write_mem_at_u8(&mem, TABLE_ADDR + 2, 0xBB);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0xBB, "AL should be from last lookup (table[2])");
}

#[test]
fn test_xlat_preserves_flags_after_zero_result() {
    // XLAT with zero result should not affect flags
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 10;
    regs.rbx = TABLE_ADDR;
    regs.rflags = 0x2 | (1 << 6); // ZF set
    let initial_flags = regs.rflags;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 10, 0); // Result is 0

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0, "AL should be 0");
    assert_eq!(final_regs.rflags, initial_flags, "Flags should not change even with 0 result");
}

#[test]
fn test_xlat_preserves_flags_after_negative_result() {
    // XLAT with high-bit set result (0x80+) should not affect flags
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 10;
    regs.rbx = TABLE_ADDR;
    regs.rflags = 0x2; // No flags set
    let initial_flags = regs.rflags;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 10, 0x80); // High bit set

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x80, "AL should be 0x80");
    assert_eq!(final_regs.rflags, initial_flags, "Flags unchanged despite high bit");
}

#[test]
fn test_xlat_multiple_with_same_index() {
    // Multiple XLAT with same AL value
    let code = [
        0xb0, 0x10, // MOV AL, 0x10
        0xd7,       // XLAT
        0xd7,       // XLAT again (use result as index)
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 0x10, 0x20); // table[0x10] = 0x20
    write_mem_at_u8(&mem, TABLE_ADDR + 0x20, 0x30); // table[0x20] = 0x30

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x30, "AL should be from second lookup");
}

#[test]
fn test_xlat_with_memory_pattern() {
    // XLAT with specific memory pattern
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 7;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    // Create a pattern in table
    for i in 0..16 {
        write_mem_at_u8(&mem, TABLE_ADDR + i, (i * 2) as u8);
    }

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 14, "AL should be 7*2=14");
}

#[test]
fn test_xlat_after_memory_operations() {
    // XLAT after other memory operations
    let code = [
        0x48, 0xa3, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // MOV [0x2000], RAX
        0xb0, 0x05, // MOV AL, 5
        0xd7,       // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x1234;
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 5, 0x77);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x77, "AL should be from table lookup");
}

#[test]
fn test_xlat_preserves_stack() {
    // XLAT should not affect stack
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0;
    regs.rbx = TABLE_ADDR;
    let initial_rsp = STACK_ADDR;
    regs.rsp = initial_rsp;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR, 0x42);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rsp, initial_rsp, "RSP unchanged");
}

#[test]
fn test_xlat_with_various_table_values() {
    // Test XLAT with various table values to ensure proper byte handling
    let test_values = [0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF];

    for (idx, &val) in test_values.iter().enumerate() {
        let code = [
            0xd7, // XLAT
            0xf4,
        ];
        let mut regs = Registers::default();
        regs.rax = idx as u64;
        regs.rbx = TABLE_ADDR;

        let (mut vcpu, mem) = setup_vm(&code, Some(regs));

        write_mem_at_u8(&mem, TABLE_ADDR + idx as u64, val);

        let final_regs = run_until_hlt(&mut vcpu).unwrap();

        assert_eq!(
            final_regs.rax & 0xFF,
            val as u64,
            "AL should match table value {}",
            val
        );
    }
}

#[test]
fn test_xlat_does_not_sign_extend() {
    // XLAT loads unsigned byte, should not sign-extend
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFFFFFFFF00; // High bits set, AL = 0
    regs.rbx = TABLE_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR, 0xFF); // Load 0xFF

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    // Only AL should be 0xFF, upper bits should remain as they were
    assert_eq!(final_regs.rax & 0xFF, 0xFF, "AL should be 0xFF");
    assert_eq!(final_regs.rax >> 8, 0xFFFFFFFFFFFF, "Upper bits unchanged");
}

#[test]
fn test_xlat_ebx_form_32bit() {
    // In 64-bit mode, XLAT uses full RBX by default
    // This test verifies basic functionality with a 32-bit-like address
    let code = [
        0xd7, // XLAT
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 3;
    regs.rbx = TABLE_ADDR; // Using a small address that fits in EBX range

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_at_u8(&mem, TABLE_ADDR + 3, 0x88);

    let final_regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(final_regs.rax & 0xFF, 0x88, "AL should be 0x88");
}
