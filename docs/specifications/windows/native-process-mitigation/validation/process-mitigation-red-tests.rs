use super::*;

#[test]
fn native_process_mitigation_filter_preserves_selector_return_length_and_padding_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, scratch) = setup(arch);
        let p = process.state_mut();
        for returned in [0, scratch + 64, scratch + 65, scratch, scratch + 4] {
            p.space.wr(scratch, &[0xA5; 128]).unwrap();
            p.space.w32(scratch, 11).unwrap();
            let before = (p.rng, p.objects.handle_count(), p.objects.iter().count());
            arguments(p, &mut t, arch.ptr(u64::MAX), 52, scratch, 8, returned);
            assert_eq!(
                dispatch(p, &mut t),
                Outcome::Continue,
                "{arch}/{returned:X}"
            );
            assert_eq!(t.cpu.gpr(0), 0);
            assert_eq!(p.space.u32(scratch).unwrap(), 11);
            assert_eq!(p.space.u32(scratch + 4).unwrap(), 0);
            assert_eq!(p.space.bytes(scratch + 8, 120).unwrap(), [0xA5; 120]);
            assert_eq!(
                (p.rng, p.objects.handle_count(), p.objects.iter().count()),
                before
            );
            assert!(t.frames.is_empty());
        }
    }
}
