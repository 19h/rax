//! Stable JSON serialization for SMIR memory addresses.

use super::{Address, OracleJson, Value, hex_u64, json};

impl OracleJson for Address {
    fn oracle_json(&self) -> Value {
        match self {
            Address::X86Addr32(inner) => {
                let mut value = inner.oracle_json();
                if let Value::Object(object) = &mut value {
                    object.insert("address_size_bits".to_string(), json!(32));
                }
                value
            }
            Address::Direct(reg) => json!({
                "kind": "direct",
                "reg": reg.oracle_json(),
            }),
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => json!({
                "kind": "base_offset",
                "base": base.oracle_json(),
                "offset": offset,
                "disp_size": disp_size.oracle_json(),
            }),
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                disp_size,
            } => json!({
                "kind": "base_index_scale",
                "base": base.oracle_json(),
                "index": index.oracle_json(),
                "scale": scale,
                "disp": disp,
                "disp_size": disp_size.oracle_json(),
            }),
            Address::PcRel {
                offset,
                disp_size,
                base,
            } => json!({
                "kind": "pc_relative",
                "offset": offset,
                "disp_size": disp_size.oracle_json(),
                "base": base.map(hex_u64),
            }),
            Address::GpRel { offset } => json!({
                "kind": "gp_relative",
                "offset": offset,
            }),
            Address::Absolute(addr) => json!({
                "kind": "absolute",
                "addr": hex_u64(*addr),
            }),
            Address::SegmentRel {
                segment,
                base,
                index,
                scale,
                disp,
            } => json!({
                "kind": "segment_rel",
                "segment": segment.oracle_json(),
                "base": base.map(|b| b.oracle_json()),
                "index": index.map(|i| i.oracle_json()),
                "scale": scale,
                "disp": disp,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::types::DispSize;
    use crate::smir::{ArchReg, VReg, X86Reg};

    #[test]
    fn addr32_preserves_address_kind_and_adds_explicit_width_marker() {
        let value = Address::X86Addr32(Box::new(Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
            index: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            scale: 4,
            disp: 0x20,
            disp_size: DispSize::Disp8,
        }))
        .oracle_json();

        assert_eq!(value["kind"], "base_index_scale");
        assert_eq!(value["address_size_bits"], 32);
        assert_eq!(value["base"]["name"], "rbx");
        assert_eq!(value["index"]["name"], "rcx");
        assert_eq!(value["scale"], 4);
        assert_eq!(value["disp"], 0x20);
    }
}
