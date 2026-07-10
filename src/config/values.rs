//! Parsed scalar configuration values.

use crate::error::{Error, Result};
use serde::Deserialize;
use serde::de::{self, Visitor};
use std::fmt;
use std::str::FromStr;

const DEFAULT_MEM_MIB: u64 = 512;

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
                    )));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address(pub u64);

impl Address {
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self> {
        let s = input.trim();
        if s.is_empty() {
            return Err(Error::InvalidConfig("address is empty".to_string()));
        }

        let value = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16)
                .map_err(|_| Error::InvalidConfig(format!("invalid address: {input}")))?
        } else {
            s.parse::<u64>()
                .map_err(|_| Error::InvalidConfig(format!("invalid address: {input}")))?
        };

        Ok(Address(value))
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct AddressVisitor;

        impl<'de> Visitor<'de> for AddressVisitor {
            type Value = Address;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("address as string or integer")
            }

            fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Address(value))
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Address::from_str(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(AddressVisitor)
    }
}
