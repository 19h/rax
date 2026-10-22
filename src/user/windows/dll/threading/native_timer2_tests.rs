//! Synthetic PE/HLE consumers share the installed-service timer state machine.
use super::*;

#[test]
fn native_timer2_arm_hle_cancellation_and_duplicate_handle_state_all_abis() {
    use crate::user::windows::objects::Object;
    for arch in WinArch::ALL {
        with_context(arch, |c, out| {
            let before = (c.p.objects.handle_count(), c.p.objects.iter().count());
            assert_eq!(integer(c, "NtCreateTimer2", &[out, 0, 0, 8, 0x1f0003]), 0);
            let handle = c.read_ptr(out).unwrap();
            let id = c.p.objects.id(handle).unwrap();
            let alias = u64::from(c.p.objects.open_access(id, false, 2).unwrap());
            c.mem().w64(out + 32, (-3_000_000_000i64) as u64).unwrap();
            assert_eq!(integer(c, "NtSetTimer2", &[alias, out + 32, 0, 0]), 0);
            assert!(matches!(
                c.p.objects.obj(id),
                Some(Object::IrTimer {
                    schedule: Some(_),
                    signaled: false,
                    ..
                })
            ));
            c.p.objects.close(handle).unwrap();
            assert_eq!(
                integer(c, "NtCancelTimer2", &[alias, arch.ptr(u64::MAX)]),
                0
            );
            assert!(matches!(
                c.p.objects.obj(id),
                Some(Object::IrTimer { schedule: None, .. })
            ));
            c.p.objects.close(alias).unwrap();
            assert!(c.p.objects.obj(id).is_none());
            assert_eq!(
                (c.p.objects.handle_count(), c.p.objects.iter().count()),
                before
            );
        });
    }
}

#[test]
fn native_timer2_arm_preserves_every_prior_ntdll_ordinal_and_trap_rva_all_abis() {
    use crate::user::windows::dll::{self, BuiltinDll};
    use crate::user::windows::hle::{Export, Item};
    use crate::user::windows::loader::builtin;
    let names = ["NtSetTimer2", "NtCancelTimer2", "NtReadVirtualMemory"];
    let current = dll::find("ntdll.dll").unwrap();
    let prior: Vec<Export> = current
        .exports
        .iter()
        .flat_map(|table| table.iter())
        .filter(|export| !names.contains(&export.name))
        .map(|export| {
            let copy = match &export.item {
                Item::Func(api) => Export::func(api.name, api.conv, api.args, api.imp),
                Item::Data(size) => Export::data(export.name, *size),
                Item::Forward(target) => Export::forward(export.name, target),
            };
            copy.only(export.archs)
        })
        .collect();
    let prior = Box::leak(prior.into_boxed_slice());
    let previous = BuiltinDll {
        name: current.name,
        display: current.display,
        subsystem: current.subsystem,
        exports: Box::leak(vec![&*prior].into_boxed_slice()),
    };
    for arch in WinArch::ALL {
        let before = builtin::build(&previous, arch, 0, &[]);
        let after = builtin::build(current, arch, 0, &[]);
        assert_eq!(after.ordinal_base, before.ordinal_base);
        assert_eq!(after.symbols.len(), before.symbols.len() + 3);
        assert_eq!(
            &after.symbols[..before.symbols.len()],
            before.symbols.as_slice()
        );
        assert_eq!(
            after.symbols[before.symbols.len()..]
                .iter()
                .map(|row| row.0)
                .collect::<Vec<_>>(),
            names
        );
    }
}
