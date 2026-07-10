//! S5L8900 firmware image loading and bootrom-call patching.

use std::fs::File;
use std::io::Read;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::config::VmConfig;
use crate::error::{Error, Result};
use crate::machine::{ArmBootInfo, BootInfo};

const IBOOT_BASE: u64 = 0x1800_0000;
const VROM_BASE: u64 = 0x2000_0000;
const LLB_BASE: u64 = 0x2200_0000;
const NOR_BASE: u64 = 0x2400_0000;
const ENGINE_8900_BASE: u32 = 0x3F00_0000;

/// Load the S5L8900 bootrom, iBoot, and NOR images, then patch the bootrom
/// function table used by the machine runtime.
pub(crate) fn load_firmware(mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo> {
    let iboot_path = &config.kernel;
    let directory = iboot_path
        .parent()
        .ok_or_else(|| Error::InvalidConfig("iboot path has no parent directory".to_string()))?;

    let read = |path: &std::path::Path| -> Result<Vec<u8>> {
        let mut data = Vec::new();
        File::open(path)?.read_to_end(&mut data)?;
        Ok(data)
    };

    let iboot = read(iboot_path)?;
    mem.write_slice(&iboot, GuestAddress(IBOOT_BASE))?;

    let bootrom_path = directory.join("bootrom_s5l8900");
    if bootrom_path.exists() {
        let bootrom = read(&bootrom_path)?;
        mem.write_slice(&bootrom, GuestAddress(VROM_BASE))?;
    }

    let nor_path = directory.join("nor_n45ap.bin");
    if nor_path.exists() {
        let nor = read(&nor_path)?;
        mem.write_slice(&nor, GuestAddress(NOR_BASE))?;
    }

    let write_u32 = |address: u64, value: u32| -> Result<()> {
        mem.write_slice(&value.to_le_bytes(), GuestAddress(address))?;
        Ok(())
    };

    write_u32(0x2000_008c, (LLB_BASE + 0x80) as u32)?;
    write_u32(0x2000_0090, (LLB_BASE + 0x100) as u32)?;
    write_u32(LLB_BASE + 0x80, 0xe3b0_0001)?;
    write_u32(LLB_BASE + 0x84, 0xe12f_ff1e)?;
    write_u32(LLB_BASE + 0x100, 0xe59f_1100)?;
    write_u32(LLB_BASE + 0x104, 0xe581_0000)?;
    write_u32(LLB_BASE + 0x108, 0xe3b0_0001)?;
    write_u32(LLB_BASE + 0x10c, 0xe12f_ff1e)?;
    write_u32(LLB_BASE + 0x208, ENGINE_8900_BASE)?;

    if std::env::var("RAX_S5L_FORCE_FSBOOT").is_ok() {
        mem.write_slice(&[0x03, 0xf0, 0xb1, 0xfd], GuestAddress(0x1800_079a))?;
    }

    tracing::info!(
        entry = format!("{:#x}", IBOOT_BASE),
        iboot = %iboot_path.display(),
        iboot_size = iboot.len(),
        bootrom = bootrom_path.exists(),
        nor = nor_path.exists(),
        "S5L8900 iBoot boot layout"
    );

    Ok(BootInfo::Arm(ArmBootInfo {
        entry_point: IBOOT_BASE,
        load_addr: IBOOT_BASE,
        image_size: iboot.len() as u64,
        dtb_addr: None,
        initial_sp: None,
    }))
}
