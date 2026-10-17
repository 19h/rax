use super::*;

#[test]
fn image_extension_owner_replays_native_protection_failures_without_copyback_drift() {
    let capture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../../docs/specifications/windows/native-image-extension-owner/native/native-image-extension-lifetime-v2.json"
    )).unwrap();
    let rows: Vec<_> = capture["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["op"] == "protect")
        .collect();
    assert_eq!(rows.len(), 216);
    let mut checked = 0;
    for arch in WinArch::ALL {
        let (mut process, mut t, scratch) = setup(arch);
        let p = process.state_mut();
        let image = image_extension_tests::template(0x7000_0000);
        let section = ImageSection::with_test_image(image.clone());
        let base = 0x5000_0000;
        p.vm.map_section_image(section, &image, base, base, 16384, 0, 8192, false, false)
            .unwrap();
        let index = p.modules.nt_services.as_ref().unwrap().0;
        p.modules.nt_services = Some((index, table(arch, "NtProtectVirtualMemory", 0x28)));
        for extension in [8192u64, 12288] {
            for row in &rows {
                let value = row["value"].as_array().unwrap();
                let protection = value[0].as_u64().unwrap();
                let offset = value[1].as_i64().unwrap();
                let size = value[2].as_u64().unwrap();
                let address = base + extension.wrapping_add_signed(offset);
                p.space.wptr(scratch, arch.ptr_size(), address).unwrap();
                p.space.wptr(scratch + 16, arch.ptr_size(), size).unwrap();
                p.space.w32(scratch + 32, 0xCCCCCCCC).unwrap();
                let status = super::super::super::super::section_map_tests::call(
                    p,
                    &mut t,
                    &[
                        arch.ptr(u64::MAX),
                        scratch,
                        scratch + 16,
                        protection,
                        scratch + 32,
                    ],
                );
                assert_eq!(u64::from(status), row["operation-status"].as_u64().unwrap());
                assert_eq!(
                    u64::from(p.space.u32(scratch + 32).unwrap()),
                    row["old"].as_u64().unwrap()
                );
                assert_eq!(p.space.ptr(scratch, arch.ptr_size()).unwrap(), address);
                assert_eq!(p.space.ptr(scratch + 16, arch.ptr_size()).unwrap(), size);
                assert_eq!(
                    p.vm.query(base + extension).unwrap().protect,
                    prot::EXECUTE_READ
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 1296);
}
