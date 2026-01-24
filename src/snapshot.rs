//! VM state snapshot for save/restore functionality.
//!
//! Supports saving and loading complete VM state including:
//! - CPU registers (general, segment, control, debug)
//! - Full memory contents (zstd compressed)
//! - Emulator-specific state (lazy flags, FPU, etc.)

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cpu::state::{CpuState, Registers, SystemRegisters, X86_64CpuState};
use crate::error::{Error, Result};

/// Magic number for snapshot files: "RAXSNAP\0"
const SNAPSHOT_MAGIC: [u8; 8] = *b"RAXSNAP\0";

/// Current snapshot format version
const SNAPSHOT_VERSION: u32 = 1;

/// x87 FPU state for snapshots
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FpuSnapshot {
    pub control_word: u16,
    pub status_word: u16,
    pub tag_word: u16,
    pub data_ptr: u64,
    pub instr_ptr: u64,
    pub last_opcode: u16,
    pub st: [f64; 8],
    pub top: u8,
}

impl Default for FpuSnapshot {
    fn default() -> Self {
        FpuSnapshot {
            control_word: 0x037F,
            status_word: 0,
            tag_word: 0xFFFF,
            data_ptr: 0,
            instr_ptr: 0,
            last_opcode: 0,
            st: [0.0; 8],
            top: 0,
        }
    }
}

/// Lazy flags state for snapshots
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LazyFlagsSnapshot {
    /// 0=None, 1=Add, 2=Sub, 3=Logic, 4=Inc, 5=Dec
    pub op: u8,
    pub result: u64,
    pub src: u64,
    pub dst: u64,
    pub size: u8,
}

impl Default for LazyFlagsSnapshot {
    fn default() -> Self {
        LazyFlagsSnapshot {
            op: 0, // None
            result: 0,
            src: 0,
            dst: 0,
            size: 4,
        }
    }
}

/// Extended CPU state specific to the emulator
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EmulatorState {
    pub fpu: FpuSnapshot,
    pub lazy_flags: LazyFlagsSnapshot,
    pub kernel_gs_base: u64,
    pub pkru: u32,
    pub halted: bool,
}

/// Complete VM snapshot
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot format version
    pub version: u32,
    /// Instruction count when snapshot was taken
    pub instruction_count: u64,
    /// CPU state
    pub cpu_state: CpuState,
    /// Extended emulator state
    pub emulator_state: EmulatorState,
    /// Memory size in bytes
    pub memory_size: u64,
    /// Compressed memory contents (zstd)
    pub memory_data: Vec<u8>,
}

impl Snapshot {
    /// Create a new snapshot from current VM state
    pub fn new(
        instruction_count: u64,
        cpu_state: CpuState,
        emulator_state: EmulatorState,
        memory: &[u8],
    ) -> Result<Self> {
        // Compress memory with zstd (level 3 for good balance of speed/compression)
        let compressed = zstd::encode_all(memory, 3)
            .map_err(|e| Error::Emulator(format!("Failed to compress memory: {}", e)))?;

        Ok(Snapshot {
            version: SNAPSHOT_VERSION,
            instruction_count,
            cpu_state,
            emulator_state,
            memory_size: memory.len() as u64,
            memory_data: compressed,
        })
    }

    /// Save snapshot to a file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path.as_ref())
            .map_err(|e| Error::Emulator(format!("Failed to create snapshot file: {}", e)))?;
        let mut writer = BufWriter::new(file);

        // Write magic number
        writer
            .write_all(&SNAPSHOT_MAGIC)
            .map_err(|e| Error::Emulator(format!("Failed to write snapshot magic: {}", e)))?;

        // Serialize snapshot with bincode
        bincode::serialize_into(&mut writer, self)
            .map_err(|e| Error::Emulator(format!("Failed to serialize snapshot: {}", e)))?;

        writer
            .flush()
            .map_err(|e| Error::Emulator(format!("Failed to flush snapshot: {}", e)))?;

        Ok(())
    }

    /// Load snapshot from a file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())
            .map_err(|e| Error::Emulator(format!("Failed to open snapshot file: {}", e)))?;
        let mut reader = BufReader::new(file);

        // Verify magic number
        let mut magic = [0u8; 8];
        reader
            .read_exact(&mut magic)
            .map_err(|e| Error::Emulator(format!("Failed to read snapshot magic: {}", e)))?;

        if magic != SNAPSHOT_MAGIC {
            return Err(Error::Emulator(
                "Invalid snapshot file (bad magic)".to_string(),
            ));
        }

        // Deserialize snapshot
        let snapshot: Snapshot = bincode::deserialize_from(&mut reader)
            .map_err(|e| Error::Emulator(format!("Failed to deserialize snapshot: {}", e)))?;

        if snapshot.version != SNAPSHOT_VERSION {
            return Err(Error::Emulator(format!(
                "Unsupported snapshot version {} (expected {})",
                snapshot.version, SNAPSHOT_VERSION
            )));
        }

        Ok(snapshot)
    }

    /// Decompress and return memory contents
    pub fn decompress_memory(&self) -> Result<Vec<u8>> {
        let mut decompressed = Vec::with_capacity(self.memory_size as usize);
        zstd::stream::copy_decode(&self.memory_data[..], &mut decompressed)
            .map_err(|e| Error::Emulator(format!("Failed to decompress memory: {}", e)))?;

        if decompressed.len() != self.memory_size as usize {
            return Err(Error::Emulator(format!(
                "Memory size mismatch: expected {} bytes, got {}",
                self.memory_size,
                decompressed.len()
            )));
        }

        Ok(decompressed)
    }

    /// Get a summary string for display
    pub fn summary(&self) -> String {
        let mem_mb = self.memory_size / (1024 * 1024);
        let compressed_mb = self.memory_data.len() / (1024 * 1024);
        let ratio = if self.memory_data.is_empty() {
            0.0
        } else {
            self.memory_size as f64 / self.memory_data.len() as f64
        };

        format!(
            "Snapshot @ insn #{}: {}MB memory ({}MB compressed, {:.1}x ratio)",
            self.instruction_count, mem_mb, compressed_mb, ratio
        )
    }
}

/// Configuration for automatic snapshotting
#[derive(Clone, Debug)]
pub struct SnapshotConfig {
    /// Take snapshot every N instructions (0 = disabled)
    pub interval: u64,
    /// Take snapshot at specific instruction counts
    pub at_instructions: Vec<u64>,
    /// Directory to save snapshots
    pub output_dir: String,
    /// Prefix for snapshot filenames
    pub prefix: String,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        SnapshotConfig {
            interval: 0,
            at_instructions: Vec::new(),
            output_dir: ".".to_string(),
            prefix: "snapshot".to_string(),
        }
    }
}

impl SnapshotConfig {
    /// Check if a snapshot should be taken at the given instruction count
    pub fn should_snapshot(&self, insn_count: u64) -> bool {
        // Check interval
        if self.interval > 0 && insn_count > 0 && insn_count % self.interval == 0 {
            return true;
        }
        // Check specific instruction counts
        self.at_instructions.contains(&insn_count)
    }

    /// Generate filename for a snapshot at the given instruction count
    pub fn filename(&self, insn_count: u64) -> String {
        format!(
            "{}/{}_{:012}.snap",
            self.output_dir, self.prefix, insn_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_config() {
        let mut config = SnapshotConfig::default();
        config.interval = 1000;
        config.at_instructions = vec![500, 1500];

        assert!(!config.should_snapshot(0));
        assert!(config.should_snapshot(500));
        assert!(config.should_snapshot(1000));
        assert!(config.should_snapshot(1500));
        assert!(config.should_snapshot(2000));
        assert!(!config.should_snapshot(1234));
    }
}
