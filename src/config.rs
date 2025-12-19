use crate::error::{Error, Result};
use clap::ValueEnum;
use serde::de::{self, Visitor};
use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

const DEFAULT_MEM_MIB: u64 = 512;
const MIN_MEM_MIB: u64 = 128;
const DEFAULT_VCPUS: u8 = 1;
const DEFAULT_CMDLINE: &str = "console=ttyS0 earlycon=uart,io,0x3f8";

#[derive(Clone, Copy, Debug, Deserialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchKind {
    X86_64,
}

impl Default for ArchKind {
    fn default() -> Self {
        ArchKind::X86_64
    }
}

#[derive(Clone, Copy, Debug, Deserialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Kvm,
    Emulator,
}

impl Default for BackendKind {
    fn default() -> Self {
        BackendKind::Kvm
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemorySize(pub u64);

impl MemorySize {
    pub fn bytes(self) -> u64 {
        self.0
    }
}

impl Default for MemorySize {
    fn default() -> Self {
        MemorySize(DEFAULT_MEM_MIB << 20)
    }
}

impl fmt::Display for MemorySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MemorySize {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self> {
        let s = input.trim();
        if s.is_empty() {
            return Err(Error::InvalidConfig("memory size is empty".to_string()));
        }

        let mut num_end = s.len();
        for (i, ch) in s.char_indices() {
            if !ch.is_ascii_digit() {
                num_end = i;
                break;
            }
        }

        let (num_part, suffix_part) = s.split_at(num_end);
        if num_part.is_empty() {
            return Err(Error::InvalidConfig(format!(
                "invalid memory size: {input}"
            )));
        }

        let value = num_part
            .parse::<u64>()
            .map_err(|_| Error::InvalidConfig(format!("invalid memory size: {input}")))?;

        let suffix = suffix_part.trim();
        let multiplier = if suffix.is_empty() {
            1u64
        } else {
            let mut suffix = suffix.to_ascii_uppercase();
            if let Some(stripped) = suffix.strip_suffix('B') {
                suffix = stripped.to_string();
            }
            if let Some(stripped) = suffix.strip_suffix('I') {
                suffix = stripped.to_string();
            }
            match suffix.as_str() {
                "K" => 1u64 << 10,
                "M" => 1u64 << 20,
                "G" => 1u64 << 30,
                "T" => 1u64 << 40,
                "P" => 1u64 << 50,
                "E" => 1u64 << 60,
                _ => {
                    return Err(Error::InvalidConfig(format!(
                        "invalid memory size suffix: {suffix_part}"
                    )))
                }
            }
        };

        let bytes = value
            .checked_mul(multiplier)
            .ok_or_else(|| Error::InvalidConfig("memory size overflow".to_string()))?;

        Ok(MemorySize(bytes))
    }
}

impl<'de> Deserialize<'de> for MemorySize {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MemorySizeVisitor;

        impl<'de> Visitor<'de> for MemorySizeVisitor {
            type Value = MemorySize;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("memory size as string or integer bytes")
            }

            fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(MemorySize(value))
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                MemorySize::from_str(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(MemorySizeVisitor)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FileConfig {
    pub arch: Option<ArchKind>,
    pub backend: Option<BackendKind>,
    pub memory: Option<MemorySize>,
    pub vcpus: Option<u8>,
    pub kernel: Option<PathBuf>,
    pub initrd: Option<PathBuf>,
    pub cmdline: Option<String>,
}

impl FileConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config = toml::from_str::<FileConfig>(&contents)
            .map_err(|e| Error::InvalidConfig(format!("toml error: {e}")))?;
        Ok(config)
    }
}

#[derive(Clone, Debug, Default)]
pub struct CliConfig {
    pub arch: Option<ArchKind>,
    pub backend: Option<BackendKind>,
    pub memory: Option<MemorySize>,
    pub vcpus: Option<u8>,
    pub kernel: Option<PathBuf>,
    pub initrd: Option<PathBuf>,
    pub cmdline: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VmConfig {
    pub arch: ArchKind,
    pub backend: BackendKind,
    pub memory: MemorySize,
    pub vcpus: u8,
    pub kernel: PathBuf,
    pub initrd: Option<PathBuf>,
    pub cmdline: String,
}

impl VmConfig {
    pub fn from_sources(cli: CliConfig, file: Option<FileConfig>) -> Result<Self> {
        let file = file.unwrap_or_default();
        let arch = cli.arch.or(file.arch).unwrap_or_default();
        let backend = cli.backend.or(file.backend).unwrap_or_default();
        let memory = cli.memory.or(file.memory).unwrap_or_default();
        let vcpus = cli.vcpus.or(file.vcpus).unwrap_or(DEFAULT_VCPUS);
        let kernel = cli
            .kernel
            .or(file.kernel)
            .ok_or_else(|| Error::InvalidConfig("kernel path is required".to_string()))?;
        let initrd = cli.initrd.or(file.initrd);
        let cmdline = cli
            .cmdline
            .or(file.cmdline)
            .unwrap_or_else(|| DEFAULT_CMDLINE.to_string());

        let config = VmConfig {
            arch,
            backend,
            memory,
            vcpus,
            kernel,
            initrd,
            cmdline,
        };

        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.vcpus == 0 {
            return Err(Error::InvalidConfig(
                "vcpus must be at least 1".to_string(),
            ));
        }
        let min_mem_bytes = MIN_MEM_MIB << 20;
        if self.memory.bytes() < min_mem_bytes {
            return Err(Error::InvalidConfig(format!(
                "memory must be at least {MIN_MEM_MIB} MiB"
            )));
        }
        if !self.kernel.exists() {
            return Err(Error::InvalidConfig(format!(
                "kernel not found: {}",
                self.kernel.display()
            )));
        }
        if let Some(initrd) = &self.initrd {
            if !initrd.exists() {
                return Err(Error::InvalidConfig(format!(
                    "initrd not found: {}",
                    initrd.display()
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_size_parses_units() {
        assert_eq!(MemorySize::from_str("1024").unwrap().bytes(), 1024);
        assert_eq!(MemorySize::from_str("1K").unwrap().bytes(), 1024);
        assert_eq!(MemorySize::from_str("1KiB").unwrap().bytes(), 1024);
        assert_eq!(MemorySize::from_str("2M").unwrap().bytes(), 2 * 1024 * 1024);
        assert_eq!(MemorySize::from_str("3g").unwrap().bytes(), 3 * 1024 * 1024 * 1024);
    }

    #[test]
    fn memory_size_rejects_bad_values() {
        assert!(MemorySize::from_str("").is_err());
        assert!(MemorySize::from_str("abc").is_err());
        assert!(MemorySize::from_str("1Z").is_err());
    }
}
