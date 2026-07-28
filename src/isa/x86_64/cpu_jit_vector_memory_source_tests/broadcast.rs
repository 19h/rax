//! CPU-level native-JIT coverage for EVEX packed FMA3 memory broadcasts.

use super::*;

fn run_to_hlt_frontier(vcpu: &mut X86_64Vcpu, frontier: u64, context: &str) {
    let mut steps = 0usize;
    while vcpu.regs.rip != frontier {
        assert!(vcpu.step().unwrap().is_none(), "{context}");
        steps += 1;
        assert!(
            steps <= 2,
            "{context}: direct execution missed HLT frontier"
        );
    }
    assert_eq!(steps, 2, "{context}");
}

#[test]
fn jit_verify_executes_unmasked_evex_packed_fma3_f32_broadcast() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping CPU JIT EVEX FMA3 broadcast: host lacks AVX-512F/BW");
        return;
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vfmadd231ps zmm17,zmm18,dword ptr [rbx+4]{1to16}; jmp next; hlt
    // Intel scalar-broadcast disp8 compression uses the 4-byte element size.
    let code = [0x62, 0xE2, 0x6D, 0x50, 0xB8, 0x4B, 0x01, 0xEB, 0x00, 0xF4];
    memory.write_slice(&code, GuestAddress(0)).unwrap();
    memory
        .write_slice(&1.5f32.to_bits().to_le_bytes(), GuestAddress(DATA_BASE + 4))
        .unwrap();

    let mut direct = long_mode_vcpu(memory.clone());
    let mut verified = long_mode_vcpu(memory);
    seed_architectural_state(&mut direct);
    seed_architectural_state(&mut verified);
    let destination = packed_f32_words(std::array::from_fn(|lane| 100.0 - lane as f32));
    let source1 = packed_f32_words(std::array::from_fn(|lane| lane as f32 * 0.5 + 2.0));
    for vcpu in [&mut direct, &mut verified] {
        vcpu.regs.zmm_ext[1] = destination;
        vcpu.regs.zmm_ext[2] = source1;
    }

    let frontier = code.len() as u64 - 1;
    run_to_hlt_frontier(&mut direct, frontier, "direct EVEX FMA3 broadcast");

    let region = verified
        .jit_compile_region()
        .expect("compile EVEX packed FMA3 broadcast region")
        .expect("helper-backed EVEX packed FMA3 broadcast must be native eligible");
    assert!(region.uses_vector);
    assert!(!region.avx_ymm16_vector_state);
    assert!(!region.narrow_vector_opmasks);

    verified.jit_run_region_verified(&region);
    assert_architectural_state_equal(
        &verified,
        &direct.regs,
        direct.mxcsr,
        "verified EVEX packed FMA3 broadcast",
    );
    assert_eq!(verified.regs.rip, frontier);
}

#[test]
fn jit_evex_packed_fma3_broadcast_fault_is_noncommitting() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping CPU JIT EVEX FMA3 broadcast fault: host lacks AVX-512F/BW");
        return;
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vfmadd231ps zmm17,zmm18,dword ptr [rbx+4]{1to16}; jmp next; hlt
    let code = [0x62, 0xE2, 0x6D, 0x50, 0xB8, 0x4B, 0x01, 0xEB, 0x00, 0xF4];
    memory.write_slice(&code, GuestAddress(0)).unwrap();

    let mut vcpu = long_mode_vcpu(memory);
    seed_architectural_state(&mut vcpu);
    vcpu.regs.rbx = 0x2_0000;
    let before = vcpu.regs.clone();
    let before_mxcsr = vcpu.mxcsr;

    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting EVEX packed FMA3 broadcast region")
        .expect("dynamic address must not prevent EVEX FMA3 broadcast admission");
    assert!(region.uses_vector);
    assert!(!region.avx_ymm16_vector_state);
    vcpu.jit_run_region_native(&region);

    assert_architectural_state_equal(
        &vcpu,
        &before,
        before_mxcsr,
        "EVEX packed FMA3 broadcast fault deoptimization",
    );
    assert_eq!(vcpu.regs.rip, 0);
}

#[test]
fn jit_verify_executes_unmasked_evex_packed_fp16_fma3_broadcast() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512fp16")
    {
        eprintln!("skipping CPU JIT EVEX FP16 FMA3 broadcast: host lacks AVX-512F/BW/FP16");
        return;
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vfmadd231ph zmm17,zmm18,word ptr [rbx+2]{1to32}; jmp next; hlt
    // Intel scalar-broadcast disp8 compression uses the 2-byte element size.
    let code = [0x62, 0xE6, 0x6D, 0x50, 0xB8, 0x4B, 0x01, 0xEB, 0x00, 0xF4];
    memory.write_slice(&code, GuestAddress(0)).unwrap();
    memory
        .write_slice(&0x3E00u16.to_le_bytes(), GuestAddress(DATA_BASE + 2))
        .unwrap();

    let mut direct = long_mode_vcpu(memory.clone());
    let mut verified = long_mode_vcpu(memory);
    seed_architectural_state(&mut direct);
    seed_architectural_state(&mut verified);
    let destination = packed_f16_words(std::array::from_fn(|lane| {
        [0x3C00, 0x4000, 0x4200, 0x4400][lane & 3]
    }));
    let source1 = packed_f16_words(std::array::from_fn(|lane| {
        [0x3800, 0x3E00, 0x4100, 0x4300][lane & 3]
    }));
    for vcpu in [&mut direct, &mut verified] {
        vcpu.regs.zmm_ext[1] = destination;
        vcpu.regs.zmm_ext[2] = source1;
    }

    let frontier = code.len() as u64 - 1;
    run_to_hlt_frontier(&mut direct, frontier, "direct EVEX FP16 FMA3 broadcast");

    let region = verified
        .jit_compile_region()
        .expect("compile EVEX packed FP16 FMA3 broadcast region")
        .expect("helper-backed EVEX packed FP16 FMA3 broadcast must be native eligible");
    assert!(region.uses_vector);
    assert!(!region.avx_ymm16_vector_state);
    assert!(!region.narrow_vector_opmasks);

    verified.jit_run_region_verified(&region);
    assert_architectural_state_equal(
        &verified,
        &direct.regs,
        direct.mxcsr,
        "verified EVEX packed FP16 FMA3 broadcast",
    );
    assert_eq!(verified.regs.rip, frontier);
}
