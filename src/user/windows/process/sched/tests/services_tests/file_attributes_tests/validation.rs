//! Semantic statuses and byte boundaries from native producer v8.

#[test]
fn native_file_attributes_wow_outputs_accept_every_byte_alignment() {
    for arch in WinArch::ALL {
        for api in ["NtQueryAttributesFile", "NtQueryFullAttributesFile"] {
            for offset in 1..8 {
                let mut h = Harness::new(arch);
                assert_eq!(
                    h.query(api, h.attrs(), h.base + offset),
                    if arch == WinArch::X86 {
                        STATUS_SUCCESS
                    } else {
                        STATUS_DATATYPE_MISALIGNMENT
                    },
                    "{arch}/{api}/byte offset {offset}"
                );
            }
        }
    }
}

use super::Harness;
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::arch::WinArch;
use crate::user::windows::memory::{Mem, prot};
use crate::user::windows::nt::status::{
    STATUS_ACCESS_VIOLATION, STATUS_DATATYPE_MISALIGNMENT, STATUS_INVALID_PARAMETER,
    STATUS_OBJECT_NAME_INVALID, STATUS_SUCCESS,
};

#[test]
fn native_file_attributes_flags_security_and_qos_match_captured_statuses() {
    for arch in WinArch::ALL {
        for api in ["NtQueryAttributesFile", "NtQueryFullAttributesFile"] {
            for bit in 0..32 {
                let mut h = Harness::new(arch);
                let attrs = h.attrs();
                h.process
                    .state()
                    .space
                    .w32(attrs + 3 * arch.ptr_size(), 1u32 << bit)
                    .unwrap();
                assert_eq!(
                    h.query(api, attrs, h.base),
                    if 0x11ff2u32 & (1u32 << bit) != 0 {
                        STATUS_SUCCESS
                    } else {
                        STATUS_INVALID_PARAMETER
                    },
                    "{arch}/{api}/flag{bit}"
                );
            }
            // Independent v8 role -> NTSTATUS mappings, including malformed
            // SID/ACL cases whose valid object name must not hide the error.
            for (role, expected) in [
                (95, 0),
                (96, 0xc0000058),
                (97, 0),
                (98, 0),
                (104, 0),
                (111, 0xc00000a5),
                (112, 0xc000000d),
                (113, 0),
                (157, 0xc0000078),
                (158, 0xc0000078),
                (159, 0xc0000077),
                (160, 0xc0000077),
                (161, 0xc0000077),
                (162, 0),
            ] {
                let mut h = Harness::new(arch);
                let attrs = h.attrs();
                let descriptor = attrs + 512;
                let child = attrs + 640;
                let p = h.process.state_mut();
                let width = arch.ptr_size();
                p.space.wr(descriptor, &[0; 80]).unwrap();
                p.space.wr(child, &[0; 80]).unwrap();
                if role >= 104 && role <= 113 {
                    p.space
                        .w32(descriptor, if role == 104 { 0 } else { 12 })
                        .unwrap();
                    p.space
                        .w32(descriptor + 4, if role == 111 { 4 } else { 2 })
                        .unwrap();
                    if role == 112 {
                        p.space.w8(descriptor + 8, 2).unwrap();
                    }
                    if role == 113 {
                        p.space.w8(descriptor + 9, 2).unwrap();
                    }
                    p.space
                        .wptr(attrs + if arch.is64() { 40 } else { 20 }, width, descriptor)
                        .unwrap();
                } else {
                    p.space
                        .w8(descriptor, if role == 96 { 0 } else { 1 })
                        .unwrap();
                    if role == 97 {
                        p.space.w16(descriptor + 2, 0x8000).unwrap();
                    }
                    if role == 98 || role >= 159 {
                        p.space.w16(descriptor + 2, 4).unwrap();
                    }
                    if role == 157 || role == 158 {
                        p.space
                            .wptr(descriptor + if arch.is64() { 8 } else { 4 }, width, child)
                            .unwrap();
                        p.space.w8(child, if role == 157 { 0 } else { 1 }).unwrap();
                        p.space
                            .w8(child + 1, if role == 158 { 16 } else { 1 })
                            .unwrap();
                    }
                    if role >= 159 {
                        p.space
                            .wptr(descriptor + if arch.is64() { 32 } else { 16 }, width, child)
                            .unwrap();
                        p.space.w8(child, if role == 159 { 0 } else { 2 }).unwrap();
                        p.space
                            .w16(
                                child + 2,
                                match role {
                                    160 => 0,
                                    161 => 7,
                                    _ => 8,
                                },
                            )
                            .unwrap();
                    }
                    p.space
                        .wptr(attrs + if arch.is64() { 32 } else { 16 }, width, descriptor)
                        .unwrap();
                }
                assert_eq!(
                    h.query(api, attrs, h.base),
                    expected,
                    "{arch}/{api}/role{role}"
                );
            }
        }
    }
}

#[test]
fn native_file_attributes_output_boundaries_match_native_and_wow_records() {
    for arch in WinArch::ALL {
        for full in [false, true] {
            for remaining in [36, 40, 52, 56] {
                let mut h = Harness::new(arch);
                let attrs = h.attrs();
                let output = h.base + PAGE_SIZE - remaining;
                h.process
                    .state_mut()
                    .vm
                    .protect(h.base + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
                let expected = if arch != WinArch::X86 && remaining % 8 != 0 {
                    STATUS_DATATYPE_MISALIGNMENT
                } else if remaining
                    < if full {
                        56
                    } else if arch == WinArch::X86 {
                        36
                    } else {
                        40
                    }
                {
                    STATUS_ACCESS_VIOLATION
                } else {
                    STATUS_SUCCESS
                };
                assert_eq!(
                    h.query(
                        if full {
                            "NtQueryFullAttributesFile"
                        } else {
                            "NtQueryAttributesFile"
                        },
                        attrs,
                        output
                    ),
                    expected,
                    "{arch}/{full}/remaining{remaining}"
                );
                if expected != STATUS_SUCCESS {
                    assert_eq!(
                        h.process
                            .state()
                            .space
                            .bytes(h.base + PAGE_SIZE - 64, 64)
                            .unwrap(),
                        [0xa5; 64]
                    );
                }
            }
            let mut h = Harness::new(arch);
            let attrs = h.attrs();
            h.process.state().space.w16(attrs + 64, 1).unwrap();
            assert_eq!(
                h.query(
                    if full {
                        "NtQueryFullAttributesFile"
                    } else {
                        "NtQueryAttributesFile"
                    },
                    attrs,
                    h.base
                ),
                STATUS_OBJECT_NAME_INVALID
            );
        }
    }
}

#[test]
fn native_file_attributes_output_aliases_preserve_captured_inputs_and_tail() {
    for arch in WinArch::ALL {
        for full in [false, true] {
            for offset in [0, 64, 128] {
                let mut h = Harness::new(arch);
                let attrs = h.attrs();
                let api = if full {
                    "NtQueryFullAttributesFile"
                } else {
                    "NtQueryAttributesFile"
                };
                assert_eq!(h.query(api, attrs, h.base), STATUS_SUCCESS);
                let copied = if full {
                    56
                } else if arch == WinArch::X86 {
                    36
                } else {
                    40
                };
                let expected = h.process.state().space.bytes(h.base, copied).unwrap();
                let output = attrs + offset;
                let tail = h
                    .process
                    .state()
                    .space
                    .bytes(output + copied as u64, 8)
                    .unwrap();
                assert_eq!(
                    h.query(api, attrs, output),
                    STATUS_SUCCESS,
                    "{arch}/{api}/alias{offset}"
                );
                assert_eq!(
                    h.process.state().space.bytes(output, copied).unwrap(),
                    expected
                );
                assert_eq!(
                    h.process
                        .state()
                        .space
                        .bytes(output + copied as u64, 8)
                        .unwrap(),
                    tail
                );
            }
        }
    }
}
