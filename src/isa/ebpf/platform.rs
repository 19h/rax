//! Platform facilities named by RFC 9669 instructions.
//!
//! RFC 9669 places the 64-bit immediate subtypes 0x1-0x6 (section 5.4) in the
//! base64 conformance group and the BTF helper call (section 4.3.1) in base32,
//! but defines their operands in terms of platform objects: maps (section
//! 5.4.1), platform variables (section 5.4.2), and helper functions. The
//! embedder binds those objects here; the engine computes every value the RFC
//! defines from them. `code_addr` needs no binding: it is the guest address of
//! an instruction of the program itself.

use std::collections::BTreeMap;

use super::{EbpfDialect, EbpfTrap, INSN_SIZE};

/// `OpKind::Syscall` key of a BTF helper call in lifted SMIR. Static helper
/// IDs occupy the low 32 bits only, so the two namespaces never alias.
pub const BTF_HELPER_SYSCALL_KEY: u64 = 1 << 32;

/// A map the program can name by file descriptor or by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EbpfMap {
    /// The value `map_by_fd(imm)` / `map_by_idx(imm)` produces.
    pub address: u64,
    /// `map_val(map)`: the address of the first value. RFC 9669 section 5.4.1
    /// defines it only for a map with a single contiguous memory region.
    pub value_address: Option<u64>,
}

/// The platform objects that LDDW subtypes 0x1, 0x2, 0x3, 0x5, and 0x6 name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EbpfPlatform {
    /// `map_by_fd(imm)`: maps by file descriptor.
    pub maps_by_fd: BTreeMap<i32, EbpfMap>,
    /// `map_by_idx(imm)`: maps by index in the program's map set.
    pub maps_by_idx: BTreeMap<i32, EbpfMap>,
    /// `var_addr(imm)`: platform variable addresses by id.
    pub variables: BTreeMap<i32, u64>,
}

impl EbpfPlatform {
    /// A platform that binds no maps or variables.
    pub const fn new() -> Self {
        Self {
            maps_by_fd: BTreeMap::new(),
            maps_by_idx: BTreeMap::new(),
            variables: BTreeMap::new(),
        }
    }

    /// The value an LDDW at `pc` loads into `dst` (RFC 9669 section 5.4,
    /// Table 12). `imm64` is `(next_imm << 32) | (u32)imm`.
    ///
    /// Solana SBPF ignores the subtype field. In RFC 9669, `next_imm` is a
    /// signed offset added to `map_val`, and `code_addr(imm)` is the
    /// instruction `imm + 1` slots after the LDDW's first slot: the Linux
    /// verifier and libbpf resolve the subtype-0x4 target as instruction
    /// index `i + imm + 1`, the same base as a program-local call. A subtype
    /// whose platform object is not bound raises `UnsupportedPseudoLoad` with
    /// the pre-instruction state intact.
    pub fn load_imm64(
        &self,
        dialect: EbpfDialect,
        pc: u64,
        src_type: u8,
        imm64: u64,
    ) -> Result<u64, EbpfTrap> {
        if dialect != EbpfDialect::Rfc9669 {
            return Ok(imm64);
        }
        let imm = imm64 as u32 as i32;
        let next_imm = i64::from((imm64 >> 32) as u32 as i32);
        let map_value = |map: Option<&EbpfMap>| {
            map.and_then(|map| map.value_address)
                .map(|value| value.wrapping_add(next_imm as u64))
        };
        let value = match src_type {
            0 => Some(imm64),
            1 => self.maps_by_fd.get(&imm).map(|map| map.address),
            2 => map_value(self.maps_by_fd.get(&imm)),
            3 => self.variables.get(&imm).copied(),
            4 => {
                let slots = i64::from(imm).wrapping_add(1);
                Some(pc.wrapping_add(slots.wrapping_mul(INSN_SIZE as i64) as u64))
            }
            5 => self.maps_by_idx.get(&imm).map(|map| map.address),
            6 => map_value(self.maps_by_idx.get(&imm)),
            _ => None,
        };
        value.ok_or(EbpfTrap::UnsupportedPseudoLoad { pc, src_type })
    }
}

/// A host helper invocation leaving lifted SMIR (`ExitReason::Syscall`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EbpfHelperKey {
    /// Helper by static ID (RFC 9669 `src_reg` 0) or an SBPF syscall key.
    Static(u32),
    /// Helper by BTF ID (RFC 9669 `src_reg` 2).
    Btf(u32),
}

impl EbpfHelperKey {
    /// Decode the `OpKind::Syscall` key the eBPF lifter emits.
    pub const fn from_smir(num: u64) -> Self {
        if num & BTF_HELPER_SYSCALL_KEY != 0 {
            Self::Btf(num as u32)
        } else {
            Self::Static(num as u32)
        }
    }

    /// The `OpKind::Syscall` key for this helper.
    pub const fn smir_key(self) -> u64 {
        match self {
            Self::Static(id) => id as u64,
            Self::Btf(id) => BTF_HELPER_SYSCALL_KEY | id as u64,
        }
    }
}
