//! Native admission requires a precise cache-maintenance source boundary.

use super::*;
use crate::smir::ir::SmirFunction;
use crate::smir::ir::ops::X86CacheMaintenanceKind;

fn function(kind: X86CacheMaintenanceKind, next_pc: u64) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::X86CacheMaintenance { kind, next_pc });
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

#[test]
fn cache_maintenance_native_admission_validates_the_instruction_boundary() {
    for (kind, length) in [
        (X86CacheMaintenanceKind::Invd, 2),
        (X86CacheMaintenanceKind::Wbinvd, 2),
        (X86CacheMaintenanceKind::Wbnoinvd, 3),
    ] {
        assert!(is_native_clobber_safe(&function(kind, 0x1000 + length)));
        for next_pc in [0, 0x1000, 0x1001, 0x1010] {
            assert!(!is_native_clobber_safe(&function(kind, next_pc)));
        }
    }
}

#[test]
fn cache_maintenance_cannot_enter_an_aarch64_state_abi_without_x86_privilege_fields() {
    let f = function(X86CacheMaintenanceKind::Wbinvd, 0x1002);
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &f,
        &std::collections::HashMap::new()
    ));
    assert!(!aarch64_gate(
        vec![f.entry_block().unwrap().ops[0].kind.clone()],
        false
    ));
}
