use rax::oracle::{
    OracleIsa, OracleMemorySeed, OracleOptions, OracleSeed, decode_to_json_with_seed,
};

#[test]
fn gs_base_msr_ir_has_no_memory_effect_even_when_the_former_boot_slot_aliases_ram() {
    let mut options = OracleOptions::default();
    options.isa = OracleIsa::X86_64;
    let target = 0x6000_u64;
    let base = target.wrapping_sub(0xffff_ffff_836e_e018);
    let seed = OracleSeed {
        regs: vec![
            ("rax".into(), u64::from(base as u32)),
            ("rcx".into(), 0xc0000101),
            ("rdx".into(), base >> 32),
            ("gs_base".into(), 0),
        ],
        memory: vec![OracleMemorySeed {
            addr: target,
            bytes: 0x200b0_u64.to_le_bytes().to_vec(),
        }],
        memory_size: Some(0x10000),
    };
    let result = decode_to_json_with_seed(&[0x0f, 0x30], &options, Some(&seed)).unwrap();
    assert_eq!(result["smir"]["available"], true);
    assert_eq!(result["smir"]["ops"][0]["memory"]["writes"], false);
    assert_eq!(result["side_effects"]["available"], true, "{result}");
    assert_eq!(
        result["side_effects"]["changed_regs"]["gs_base"]["after"],
        format!("0x{base:x}")
    );
    assert!(
        result["side_effects"]["changed_memory"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
