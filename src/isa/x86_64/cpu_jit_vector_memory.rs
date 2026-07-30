//! Fault-precise x86 native-JIT vector-memory helpers.

use super::X86_64Vcpu;
use crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX;
use crate::smir::lower::runtime::GuestRegs;

/// Read one complete vector operand before modifying an architectural ZMM slot,
/// or a 4/8/16/32/64-byte fusion operand into the reserved nonarchitectural
/// scratch. Architectural indices are 0..=31; index 32 names only
/// `GuestRegs::vector_scratch`.
pub(super) unsafe extern "C" fn rax_jit_vec_load(
    state: *mut GuestRegs,
    addr: u64,
    dst_idx: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let scratch = dst_idx == X86_JIT_VECTOR_SCRATCH_INDEX;
    let size_valid = if scratch {
        matches!(size, 4 | 8 | 16 | 32 | 64)
    } else {
        matches!(size, 16 | 32 | 64)
    };
    if dst_idx > X86_JIT_VECTOR_SCRATCH_INDEX || !size_valid {
        return 0;
    }
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    let Ok(bytes) = vcpu.read_bytes(addr, size as usize) else {
        return 0;
    };

    let destination = if dst_idx == X86_JIT_VECTOR_SCRATCH_INDEX {
        &mut state.vector_scratch
    } else {
        &mut state.zmm[dst_idx as usize]
    };
    let mut value = if zero_upper != 0 {
        [0u64; 8]
    } else {
        *destination
    };
    for (word, chunk) in value.iter_mut().zip(bytes.chunks(8)) {
        let mut word_bytes = word.to_le_bytes();
        word_bytes[..chunk.len()].copy_from_slice(chunk);
        *word = u64::from_le_bytes(word_bytes);
    }
    *destination = value;
    1
}

/// Copy source bytes from one architectural ZMM slot or the reserved
/// nonarchitectural transfer scratch, then perform one complete guest-memory
/// write. Architectural indices are 0..=31; index 32 names only
/// `GuestRegs::vector_scratch`.
pub(super) unsafe extern "C" fn rax_jit_vec_store(
    state: *mut GuestRegs,
    addr: u64,
    src_idx: u32,
    size: u32,
) -> u64 {
    if src_idx > X86_JIT_VECTOR_SCRATCH_INDEX || !matches!(size, 4 | 8 | 16 | 32 | 64) {
        return 0;
    }
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    let Some(last) = addr.checked_add(u64::from(size) - 1) else {
        return 0;
    };
    if vcpu.mmu.is_code_page(addr) || vcpu.mmu.is_code_page(last) {
        return 0;
    }

    let source = if src_idx == X86_JIT_VECTOR_SCRATCH_INDEX {
        &state.vector_scratch
    } else {
        &state.zmm[src_idx as usize]
    };
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes[..size as usize].chunks_mut(8).zip(source) {
        let word = word.to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }

    if vcpu.jit_mem_log.is_some() {
        match vcpu.read_bytes(addr, size as usize) {
            Ok(old) => {
                for (offset, chunk) in old.chunks(8).enumerate() {
                    let mut value = [0u8; 8];
                    value[..chunk.len()].copy_from_slice(chunk);
                    vcpu.push_jit_mem_log((
                        addr.wrapping_add((offset * 8) as u64),
                        chunk.len() as u8,
                        u64::from_le_bytes(value),
                    ));
                    if vcpu.jit_mem_log.is_none() {
                        break;
                    }
                }
            }
            Err(_) => vcpu.jit_mem_log = None,
        }
    }

    u64::from(vcpu.write_bytes(addr, &bytes[..size as usize]).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

    #[test]
    fn helpers_preserve_lane_semantics_scratch_isolation_fault_atomicity_and_store_undo() {
        let mem =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        let mut vcpu = X86_64Vcpu::new(0, mem.clone());
        let source: Vec<u8> = (0..64).map(|byte| byte as u8 ^ 0xA5).collect();
        mem.write_slice(&source, GuestAddress(0x2000)).unwrap();

        let mut state = GuestRegs::default();
        state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
        state.zmm[3] = [0xDEAD_BEEF_CAFE_BABE; 8];
        assert_eq!(unsafe { rax_jit_vec_load(&mut state, 0x2000, 3, 16, 0) }, 1);
        assert_eq!(
            state.zmm[3][0],
            u64::from_le_bytes(source[0..8].try_into().unwrap())
        );
        assert_eq!(
            state.zmm[3][1],
            u64::from_le_bytes(source[8..16].try_into().unwrap())
        );
        assert_eq!(state.zmm[3][2..], [0xDEAD_BEEF_CAFE_BABE; 6]);

        state.zmm[3] = [u64::MAX; 8];
        assert_eq!(unsafe { rax_jit_vec_load(&mut state, 0x2000, 3, 32, 1) }, 1);
        for (word, chunk) in state.zmm[3][..4].iter().zip(source[..32].chunks_exact(8)) {
            assert_eq!(*word, u64::from_le_bytes(chunk.try_into().unwrap()));
        }
        assert_eq!(state.zmm[3][4..], [0; 4]);

        let architectural_before = state.zmm;
        state.vector_scratch = [0x0123_4567_89AB_CDEF; 8];
        assert_eq!(
            unsafe { rax_jit_vec_load(&mut state, 0x2000, X86_JIT_VECTOR_SCRATCH_INDEX, 16, 1,) },
            1
        );
        assert_eq!(state.zmm, architectural_before);
        assert_eq!(
            state.vector_scratch[..2],
            [
                u64::from_le_bytes(source[0..8].try_into().unwrap()),
                u64::from_le_bytes(source[8..16].try_into().unwrap()),
            ]
        );
        assert_eq!(state.vector_scratch[2..], [0; 6]);

        for size in [4, 8] {
            state.vector_scratch = [u64::MAX; 8];
            assert_eq!(
                unsafe {
                    rax_jit_vec_load(&mut state, 0x2000, X86_JIT_VECTOR_SCRATCH_INDEX, size, 1)
                },
                1
            );
            let mut expected = [0u8; 64];
            expected[..size as usize].copy_from_slice(&source[..size as usize]);
            let actual: Vec<_> = state
                .vector_scratch
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect();
            assert_eq!(actual, expected, "{size}-byte scratch load");
        }

        let scratch_before_fault = state.vector_scratch;
        assert_eq!(
            unsafe { rax_jit_vec_load(&mut state, 0xFFFF, X86_JIT_VECTOR_SCRATCH_INDEX, 64, 1,) },
            0
        );
        assert_eq!(
            state.vector_scratch, scratch_before_fault,
            "faulting scratch load must not commit"
        );
        let before_fault = state.zmm[3];
        assert_eq!(unsafe { rax_jit_vec_load(&mut state, 0xFFFF, 3, 64, 1) }, 0);
        assert_eq!(state.zmm[3], before_fault, "faulting load must not commit");
        assert_eq!(
            unsafe {
                rax_jit_vec_load(&mut state, 0x2000, X86_JIT_VECTOR_SCRATCH_INDEX + 1, 16, 0)
            },
            0
        );
        assert_eq!(unsafe { rax_jit_vec_load(&mut state, 0x2000, 3, 8, 0) }, 0);

        state.zmm[31] = [
            0x0706_0504_0302_0100,
            0x0F0E_0D0C_0B0A_0908,
            0x1716_1514_1312_1110,
            0x1F1E_1D1C_1B1A_1918,
            0x2726_2524_2322_2120,
            0x2F2E_2D2C_2B2A_2928,
            0x3736_3534_3332_3130,
            0x3F3E_3D3C_3B3A_3938,
        ];
        let old = vec![0xCC; 64];
        mem.write_slice(&old, GuestAddress(0x3000)).unwrap();
        assert_eq!(unsafe { rax_jit_vec_store(&mut state, 0x3000, 31, 64) }, 1);
        let mut stored = [0u8; 64];
        mem.read_slice(&mut stored, GuestAddress(0x3000)).unwrap();
        assert_eq!(stored, std::array::from_fn::<_, 64, _>(|index| index as u8));

        mem.write_slice(&old[..32], GuestAddress(0x3000)).unwrap();
        vcpu.jit_mem_log = Some(Vec::new());
        assert_eq!(unsafe { rax_jit_vec_store(&mut state, 0x3000, 31, 32) }, 1);
        let log = vcpu.jit_mem_log.take().unwrap();
        assert_eq!(log.len(), 4);
        for (index, &(addr, size, value)) in log.iter().enumerate() {
            assert_eq!(addr, 0x3000 + index as u64 * 8);
            assert_eq!(size, 8);
            assert_eq!(value, 0xCCCC_CCCC_CCCC_CCCC);
        }

        for (addr, size, expected_prefix) in [
            (0x3100, 4_u32, &[0, 1, 2, 3][..]),
            (0x3110, 8_u32, &[0, 1, 2, 3, 4, 5, 6, 7][..]),
        ] {
            mem.write_slice(&old[..8], GuestAddress(addr)).unwrap();
            vcpu.jit_mem_log = Some(Vec::new());
            assert_eq!(unsafe { rax_jit_vec_store(&mut state, addr, 31, size) }, 1);
            let mut scalar = [0u8; 8];
            mem.read_slice(&mut scalar, GuestAddress(addr)).unwrap();
            assert_eq!(&scalar[..size as usize], expected_prefix);
            assert_eq!(&scalar[size as usize..], &old[size as usize..8]);
            assert_eq!(
                vcpu.jit_mem_log.take().unwrap(),
                vec![(addr, size as u8, 0xCCCC_CCCC_CCCC_CCCC >> (64 - size * 8))]
            );
        }

        state.vector_scratch = [
            0x8877_6655_4433_2211,
            0xFFEE_DDCC_BBAA_0099,
            0x0123_4567_89AB_CDEF,
            0xFEDC_BA98_7654_3210,
            0,
            1,
            2,
            3,
        ];
        let architectural_before = state.zmm;
        mem.write_slice(&old[..8], GuestAddress(0x3120)).unwrap();
        assert_eq!(
            unsafe { rax_jit_vec_store(&mut state, 0x3120, X86_JIT_VECTOR_SCRATCH_INDEX, 8,) },
            1
        );
        let mut scratch_stored = [0u8; 8];
        mem.read_slice(&mut scratch_stored, GuestAddress(0x3120))
            .unwrap();
        assert_eq!(scratch_stored, 0x8877_6655_4433_2211u64.to_le_bytes());
        assert_eq!(
            state.zmm, architectural_before,
            "scratch store must not modify architectural vectors"
        );

        vcpu.mmu.mark_code_page(0x6000);
        let protected = [0x55u8; 16];
        mem.write_slice(&protected, GuestAddress(0x5FF8)).unwrap();
        assert_eq!(unsafe { rax_jit_vec_store(&mut state, 0x5FF8, 31, 16) }, 0);
        assert_eq!(
            unsafe { rax_jit_vec_store(&mut state, 0x5FF8, X86_JIT_VECTOR_SCRATCH_INDEX, 16,) },
            0
        );
        let mut unchanged = [0u8; 16];
        mem.read_slice(&mut unchanged, GuestAddress(0x5FF8))
            .unwrap();
        assert_eq!(unchanged, protected, "either covered code page must deopt");
        assert_eq!(
            unsafe { rax_jit_vec_store(&mut state, 0x3000, X86_JIT_VECTOR_SCRATCH_INDEX + 1, 16,) },
            0
        );
        assert_eq!(unsafe { rax_jit_vec_store(&mut state, 0x3000, 0, 2) }, 0);
        assert_eq!(
            unsafe { rax_jit_vec_load(std::ptr::null_mut(), 0, 0, 16, 0) },
            0
        );
        assert_eq!(
            unsafe { rax_jit_vec_store(std::ptr::null_mut(), 0, 0, 16) },
            0
        );
    }
}
