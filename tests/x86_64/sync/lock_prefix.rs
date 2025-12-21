use crate::common::{run_until_hlt, setup_vm, read_mem_u8, read_mem_u16, read_mem_u32, read_mem_u64, write_mem_u8, write_mem_u16, write_mem_u32, write_mem_u64};

// LOCK Prefix Tests - Comprehensive tests for LOCK prefix with various instructions
// The LOCK prefix (0xF0) ensures atomic execution on multiprocessor systems

// ===== LOCK ADD TESTS =====

#[test]
fn test_lock_add_8bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x80, 0x03, 0x05,                   // LOCK ADD BYTE PTR [RBX], 5
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u8(&mem, 10);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u8(&mem), 15, "Memory should be atomically incremented");
}

#[test]
fn test_lock_add_16bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x66, 0x81, 0x03, 0xe8, 0x03,       // LOCK ADD WORD PTR [RBX], 1000
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u16(&mem, 5000);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u16(&mem), 6000, "Memory should be atomically incremented");
}

#[test]
fn test_lock_add_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x81, 0x03, 0x64, 0x00, 0x00, 0x00, // LOCK ADD DWORD PTR [RBX], 100
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 1000);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 1100, "Memory should be atomically incremented");
}

#[test]
fn test_lock_add_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00,             // MOV RBX, 0x2000
        0xf0, 0x48, 0x81, 0x03, 0x10, 0x27, 0x00, 0x00,       // LOCK ADD QWORD PTR [RBX], 10000
        0xf4,                                                 // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 100000);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 110000, "Memory should be atomically incremented");
}

// ===== LOCK SUB TESTS =====

#[test]
fn test_lock_sub_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x81, 0x2b, 0x32, 0x00, 0x00, 0x00, // LOCK SUB DWORD PTR [RBX], 50
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 200);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 150, "Memory should be atomically decremented");
}

#[test]
fn test_lock_sub_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00,             // MOV RBX, 0x2000
        0xf0, 0x48, 0x81, 0x2b, 0xe8, 0x03, 0x00, 0x00,       // LOCK SUB QWORD PTR [RBX], 1000
        0xf4,                                                 // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 5000);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 4000, "Memory should be atomically decremented");
}

// ===== LOCK AND TESTS =====

#[test]
fn test_lock_and_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x81, 0x23, 0x0f, 0x00, 0x00, 0x00, // LOCK AND DWORD PTR [RBX], 0x0F
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0xFF);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0x0F, "Memory should be atomically ANDed");
}

#[test]
fn test_lock_and_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00,                       // MOV RBX, 0x2000
        0xf0, 0x48, 0x81, 0x23, 0xff, 0xff, 0x00, 0x00,                 // LOCK AND QWORD PTR [RBX], 0xFFFF
        0xf4,                                                           // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 0x123456789ABCDEF);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 0xCDEF, "Memory should be atomically ANDed");
}

// ===== LOCK OR TESTS =====

#[test]
fn test_lock_or_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x81, 0x0b, 0xf0, 0x00, 0x00, 0x00, // LOCK OR DWORD PTR [RBX], 0xF0
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0x0F);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0xFF, "Memory should be atomically ORed");
}

#[test]
fn test_lock_or_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00,                       // MOV RBX, 0x2000
        0xf0, 0x48, 0x81, 0x0b, 0x00, 0x00, 0xff, 0x00,                 // LOCK OR QWORD PTR [RBX], 0xFF0000
        0xf4,                                                           // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 0x12345678);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 0x12FF5678, "Memory should be atomically ORed");
}

// ===== LOCK XOR TESTS =====

#[test]
fn test_lock_xor_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x81, 0x33, 0xff, 0xff, 0xff, 0xff, // LOCK XOR DWORD PTR [RBX], 0xFFFFFFFF
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0x12345678);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0xEDCBA987, "Memory should be atomically XORed");
}

#[test]
fn test_lock_xor_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00,                       // MOV RBX, 0x2000
        0xf0, 0x48, 0x81, 0x33, 0xff, 0xff, 0xff, 0xff,                 // LOCK XOR QWORD PTR [RBX], 0xFFFFFFFF
        0xf4,                                                           // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 0x1234567890ABCDEF);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 0x12345678_6F543210, "Memory should be atomically XORed");
}

// ===== LOCK INC/DEC TESTS =====

#[test]
fn test_lock_inc_8bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0xfe, 0x03,                         // LOCK INC BYTE PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u8(&mem, 99);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u8(&mem), 100, "Memory should be atomically incremented");
}

#[test]
fn test_lock_inc_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0xff, 0x03,                         // LOCK INC DWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 999);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 1000, "Memory should be atomically incremented");
}

#[test]
fn test_lock_dec_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0xff, 0x0b,                         // LOCK DEC DWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 1001);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 1000, "Memory should be atomically decremented");
}

#[test]
fn test_lock_inc_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x48, 0xff, 0x03,                   // LOCK INC QWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 9999999);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 10000000, "Memory should be atomically incremented");
}

#[test]
fn test_lock_dec_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x48, 0xff, 0x0b,                   // LOCK DEC QWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 10000001);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 10000000, "Memory should be atomically decremented");
}

// ===== LOCK NEG TESTS =====

#[test]
fn test_lock_neg_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0xf7, 0x1b,                         // LOCK NEG DWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 42);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem) as i32, -42, "Memory should be atomically negated");
}

#[test]
fn test_lock_neg_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x48, 0xf7, 0x1b,                   // LOCK NEG QWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 1000);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem) as i64, -1000, "Memory should be atomically negated");
}

// ===== LOCK NOT TESTS =====

#[test]
fn test_lock_not_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0xf7, 0x13,                         // LOCK NOT DWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0x12345678);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0xEDCBA987, "Memory should be atomically NOTed");
}

#[test]
fn test_lock_not_64bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x48, 0xf7, 0x13,                   // LOCK NOT QWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 0x123456789ABCDEF0);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 0xEDCBA98765432110 - 1, "Memory should be atomically NOTed");
}

// ===== LOCK BTC/BTR/BTS TESTS =====

#[test]
fn test_lock_bts_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x0f, 0xba, 0x2b, 0x08,             // LOCK BTS DWORD PTR [RBX], 8
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0x100, "Bit 8 should be atomically set");
}

#[test]
fn test_lock_btr_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x0f, 0xba, 0x33, 0x08,             // LOCK BTR DWORD PTR [RBX], 8
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0xFFFFFFFF);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0xFFFFFEFF, "Bit 8 should be atomically reset");
}

#[test]
fn test_lock_btc_32bit_memory() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        0xf0, 0x0f, 0xba, 0x3b, 0x08,             // LOCK BTC DWORD PTR [RBX], 8
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0x100, "Bit 8 should be atomically complemented");
}

// ===== PRACTICAL ATOMIC PATTERNS =====

#[test]
fn test_lock_sequence_counter_increment() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        // Multiple atomic increments
        0xf0, 0x48, 0xff, 0x03,                   // LOCK INC QWORD PTR [RBX]
        0xf0, 0x48, 0xff, 0x03,                   // LOCK INC QWORD PTR [RBX]
        0xf0, 0x48, 0xff, 0x03,                   // LOCK INC QWORD PTR [RBX]
        0xf0, 0x48, 0xff, 0x03,                   // LOCK INC QWORD PTR [RBX]
        0xf0, 0x48, 0xff, 0x03,                   // LOCK INC QWORD PTR [RBX]
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 100);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 105, "Counter should be incremented 5 times");
}

#[test]
fn test_lock_flags_operations() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        // Set flag bit 0
        0xf0, 0x0f, 0xba, 0x2b, 0x00,             // LOCK BTS DWORD PTR [RBX], 0
        // Set flag bit 1
        0xf0, 0x0f, 0xba, 0x2b, 0x01,             // LOCK BTS DWORD PTR [RBX], 1
        // Set flag bit 7
        0xf0, 0x0f, 0xba, 0x2b, 0x07,             // LOCK BTS DWORD PTR [RBX], 7
        0xf4,                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0x83, "Bits 0, 1, and 7 should be set");
}

#[test]
fn test_lock_reference_count_pattern() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00,             // MOV RBX, 0x2000
        // Add 3 references
        0xf0, 0x48, 0x81, 0x03, 0x03, 0x00, 0x00, 0x00,       // LOCK ADD QWORD PTR [RBX], 3
        // Remove 1 reference
        0xf0, 0x48, 0x81, 0x2b, 0x01, 0x00, 0x00, 0x00,       // LOCK SUB QWORD PTR [RBX], 1
        0xf4,                                                 // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 1);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 3, "Reference count should be 3");
}

#[test]
fn test_lock_bitmask_operations() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00,                 // MOV RBX, 0x2000
        // Set some bits
        0xf0, 0x81, 0x0b, 0x0f, 0xf0, 0x00, 0x00,                 // LOCK OR DWORD PTR [RBX], 0xF00F
        // Clear some bits
        0xf0, 0x81, 0x23, 0xf0, 0x0f, 0xff, 0xff,                 // LOCK AND DWORD PTR [RBX], 0xFFFF0FF0
        0xf4,                                                     // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0xF000, "Bitmask should be applied atomically");
}

#[test]
fn test_lock_accumulator_pattern() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00,             // MOV RBX, 0x2000
        0xf0, 0x48, 0x81, 0x03, 0x0a, 0x00, 0x00, 0x00,       // LOCK ADD QWORD PTR [RBX], 10
        0xf0, 0x48, 0x81, 0x03, 0x14, 0x00, 0x00, 0x00,       // LOCK ADD QWORD PTR [RBX], 20
        0xf0, 0x48, 0x81, 0x03, 0x1e, 0x00, 0x00, 0x00,       // LOCK ADD QWORD PTR [RBX], 30
        0xf4,                                                 // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 0);
    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 60, "Accumulator should sum all values");
}
