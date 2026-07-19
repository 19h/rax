//! JSON representation of SMIR call targets.

use serde_json::{Value, json};

use super::{OracleJson, hex_u64};
use crate::smir::CallTarget;

pub(super) fn call_target_json(target: &CallTarget) -> Value {
    match target {
        CallTarget::Direct(id) => json!({"kind": "direct_function", "id": id.0}),
        CallTarget::GuestAddr(addr) => json!({"kind": "direct", "addr": hex_u64(*addr)}),
        CallTarget::GuestAddrInterworking { addr, thumb } => json!({
            "kind": "direct_interworking",
            "addr": hex_u64(*addr),
            "thumb": thumb,
        }),
        CallTarget::Indirect(reg) => json!({
            "kind": "indirect",
            "reg": format!("{reg:?}"),
            "reg_value": reg.oracle_json(),
        }),
        CallTarget::IndirectInterworking(reg) => json!({
            "kind": "indirect_interworking",
            "reg": format!("{reg:?}"),
            "reg_value": reg.oracle_json(),
        }),
        CallTarget::IndirectMem(addr) => json!({
            "kind": "indirect_mem",
            "addr": format!("{addr:?}"),
            "address": addr.oracle_json(),
        }),
        CallTarget::X86IndirectMemAddr32(addr) => json!({
            "kind": "indirect_mem",
            "address_size_bits": 32,
            "addr": format!("{addr:?}"),
            "address": addr.oracle_json(),
        }),
        CallTarget::Runtime(func) => json!({"kind": "runtime", "func": format!("{func:?}")}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::{Address, ArchReg, VReg, X86Reg};

    #[test]
    fn addr32_memory_target_retains_c_api_compatible_kind_and_width_marker() {
        let value = call_target_json(&CallTarget::X86IndirectMemAddr32(Address::Direct(
            VReg::Arch(ArchReg::X86(X86Reg::Rax)),
        )));

        assert_eq!(value["kind"], "indirect_mem");
        assert_eq!(value["address_size_bits"], 32);
        assert!(value.get("address").is_some());
    }
}
