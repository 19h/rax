//! ARM ELF loader for loading and executing ARM binaries.
//!
//! This module provides functionality to:
//! - Parse ELF headers for ARM (32-bit) binaries
//! - Load program segments into memory
//! - Set up initial CPU state for execution
//! - Handle both static and dynamic executables
//!
//! # Supported Features
//!
//! - ELF32 ARM (EM_ARM = 40)
//! - Little-endian format
//! - Executable and shared object files
//! - PT_LOAD segments
//! - Initial stack and argument setup
//!
//! # Usage
//!
//! ```ignore
//! use rax::arm::elf::ElfLoader;
//! use rax::arm::{Armv7Cpu, FlatMemory};
//!
//! let elf_data = std::fs::read("program.elf")?;
//! let mut cpu = Armv7Cpu::new();
//! let mut mem = FlatMemory::new();
//!
//! let loader = ElfLoader::parse(&elf_data)?;
//! loader.load(&mut mem)?;
//! loader.setup_cpu(&mut cpu, &["arg0", "arg1"], &["PATH=/usr"]);
//! ```

use std::fmt;
use crate::arm::execution::{Armv7Cpu, ArmMemory, MemoryError};

/// ELF magic number.
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class for 32-bit.
const ELFCLASS32: u8 = 1;

/// ELF data encoding for little-endian.
const ELFDATA2LSB: u8 = 1;

/// ELF type: Executable.
const ET_EXEC: u16 = 2;
/// ELF type: Shared object.
const ET_DYN: u16 = 3;

/// Machine type for ARM.
const EM_ARM: u16 = 40;

/// Program header type: Loadable segment.
const PT_LOAD: u32 = 1;
/// Program header type: Interpreter path.
const PT_INTERP: u32 = 3;
/// Program header type: Program header table.
const PT_PHDR: u32 = 6;

/// Segment flags: Executable.
const PF_X: u32 = 1;
/// Segment flags: Writable.
const PF_W: u32 = 2;
/// Segment flags: Readable.
const PF_R: u32 = 4;

/// ELF parsing errors.
#[derive(Clone, Debug)]
pub enum ElfError {
    /// File is too short.
    TooShort,
    /// Invalid ELF magic number.
    InvalidMagic,
    /// Not a 32-bit ELF.
    Not32Bit,
    /// Not little-endian.
    NotLittleEndian,
    /// Not an ARM binary.
    NotArm,
    /// Not an executable or shared object.
    NotExecutable,
    /// Invalid program header.
    InvalidProgramHeader,
    /// Memory error during loading.
    MemoryError(MemoryError),
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ElfError::TooShort => write!(f, "ELF file too short"),
            ElfError::InvalidMagic => write!(f, "Invalid ELF magic number"),
            ElfError::Not32Bit => write!(f, "Not a 32-bit ELF"),
            ElfError::NotLittleEndian => write!(f, "Not little-endian"),
            ElfError::NotArm => write!(f, "Not an ARM binary"),
            ElfError::NotExecutable => write!(f, "Not an executable or shared object"),
            ElfError::InvalidProgramHeader => write!(f, "Invalid program header"),
            ElfError::MemoryError(e) => write!(f, "Memory error: {:?}", e),
        }
    }
}

impl std::error::Error for ElfError {}

impl From<MemoryError> for ElfError {
    fn from(e: MemoryError) -> Self {
        ElfError::MemoryError(e)
    }
}

/// Parsed ELF32 header.
#[derive(Clone, Debug)]
pub struct Elf32Header {
    /// ELF type (executable, shared object, etc.).
    pub e_type: u16,
    /// Machine type.
    pub e_machine: u16,
    /// Entry point address.
    pub e_entry: u32,
    /// Program header table offset.
    pub e_phoff: u32,
    /// Section header table offset.
    pub e_shoff: u32,
    /// Processor-specific flags.
    pub e_flags: u32,
    /// Size of this header.
    pub e_ehsize: u16,
    /// Size of program header entry.
    pub e_phentsize: u16,
    /// Number of program headers.
    pub e_phnum: u16,
    /// Size of section header entry.
    pub e_shentsize: u16,
    /// Number of section headers.
    pub e_shnum: u16,
    /// Section header string table index.
    pub e_shstrndx: u16,
}

/// Parsed ELF32 program header.
#[derive(Clone, Debug)]
pub struct Elf32ProgramHeader {
    /// Segment type.
    pub p_type: u32,
    /// Offset in file.
    pub p_offset: u32,
    /// Virtual address.
    pub p_vaddr: u32,
    /// Physical address.
    pub p_paddr: u32,
    /// Size in file.
    pub p_filesz: u32,
    /// Size in memory.
    pub p_memsz: u32,
    /// Segment flags.
    pub p_flags: u32,
    /// Alignment.
    pub p_align: u32,
}

impl Elf32ProgramHeader {
    /// Check if segment is executable.
    pub fn is_executable(&self) -> bool {
        (self.p_flags & PF_X) != 0
    }
    
    /// Check if segment is writable.
    pub fn is_writable(&self) -> bool {
        (self.p_flags & PF_W) != 0
    }
    
    /// Check if segment is readable.
    pub fn is_readable(&self) -> bool {
        (self.p_flags & PF_R) != 0
    }
}

/// ELF loader for ARM binaries.
#[derive(Clone, Debug)]
pub struct ElfLoader<'a> {
    /// Raw ELF data.
    data: &'a [u8],
    /// Parsed ELF header.
    pub header: Elf32Header,
    /// Program headers.
    pub program_headers: Vec<Elf32ProgramHeader>,
    /// Is this a PIE/shared object?
    pub is_pie: bool,
    /// Base address for PIE loading.
    pub load_base: u32,
}

impl<'a> ElfLoader<'a> {
    /// Parse an ELF file from raw bytes.
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.len() < 52 {
            return Err(ElfError::TooShort);
        }
        
        // Check ELF magic.
        if data[0..4] != ELF_MAGIC {
            return Err(ElfError::InvalidMagic);
        }
        
        // Check class (32-bit).
        if data[4] != ELFCLASS32 {
            return Err(ElfError::Not32Bit);
        }
        
        // Check endianness (little-endian).
        if data[5] != ELFDATA2LSB {
            return Err(ElfError::NotLittleEndian);
        }
        
        // Parse header.
        let e_type = u16::from_le_bytes([data[16], data[17]]);
        let e_machine = u16::from_le_bytes([data[18], data[19]]);
        
        // Check machine type.
        if e_machine != EM_ARM {
            return Err(ElfError::NotArm);
        }
        
        // Check file type.
        let is_pie = e_type == ET_DYN;
        if e_type != ET_EXEC && e_type != ET_DYN {
            return Err(ElfError::NotExecutable);
        }
        
        let header = Elf32Header {
            e_type,
            e_machine,
            e_entry: u32::from_le_bytes([data[24], data[25], data[26], data[27]]),
            e_phoff: u32::from_le_bytes([data[28], data[29], data[30], data[31]]),
            e_shoff: u32::from_le_bytes([data[32], data[33], data[34], data[35]]),
            e_flags: u32::from_le_bytes([data[36], data[37], data[38], data[39]]),
            e_ehsize: u16::from_le_bytes([data[40], data[41]]),
            e_phentsize: u16::from_le_bytes([data[42], data[43]]),
            e_phnum: u16::from_le_bytes([data[44], data[45]]),
            e_shentsize: u16::from_le_bytes([data[46], data[47]]),
            e_shnum: u16::from_le_bytes([data[48], data[49]]),
            e_shstrndx: u16::from_le_bytes([data[50], data[51]]),
        };
        
        // Parse program headers.
        let mut program_headers = Vec::with_capacity(header.e_phnum as usize);
        for i in 0..header.e_phnum as usize {
            let offset = header.e_phoff as usize + i * header.e_phentsize as usize;
            if offset + 32 > data.len() {
                return Err(ElfError::InvalidProgramHeader);
            }
            
            let phdr = Elf32ProgramHeader {
                p_type: u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]),
                p_offset: u32::from_le_bytes([data[offset+4], data[offset+5], data[offset+6], data[offset+7]]),
                p_vaddr: u32::from_le_bytes([data[offset+8], data[offset+9], data[offset+10], data[offset+11]]),
                p_paddr: u32::from_le_bytes([data[offset+12], data[offset+13], data[offset+14], data[offset+15]]),
                p_filesz: u32::from_le_bytes([data[offset+16], data[offset+17], data[offset+18], data[offset+19]]),
                p_memsz: u32::from_le_bytes([data[offset+20], data[offset+21], data[offset+22], data[offset+23]]),
                p_flags: u32::from_le_bytes([data[offset+24], data[offset+25], data[offset+26], data[offset+27]]),
                p_align: u32::from_le_bytes([data[offset+28], data[offset+29], data[offset+30], data[offset+31]]),
            };
            program_headers.push(phdr);
        }
        
        // Default load base for PIE.
        let load_base = if is_pie { 0x0010_0000 } else { 0 };
        
        Ok(ElfLoader {
            data,
            header,
            program_headers,
            is_pie,
            load_base,
        })
    }
    
    /// Set the load base address for PIE binaries.
    pub fn set_load_base(&mut self, base: u32) {
        self.load_base = base;
    }
    
    /// Get the entry point address (adjusted for PIE if needed).
    pub fn entry_point(&self) -> u32 {
        if self.is_pie {
            self.load_base.wrapping_add(self.header.e_entry)
        } else {
            self.header.e_entry
        }
    }
    
    /// Load program segments into memory.
    pub fn load<M: ArmMemory>(&self, mem: &mut M) -> Result<(), ElfError> {
        for phdr in &self.program_headers {
            if phdr.p_type != PT_LOAD {
                continue;
            }
            
            let vaddr = if self.is_pie {
                self.load_base.wrapping_add(phdr.p_vaddr)
            } else {
                phdr.p_vaddr
            };
            
            // Load file contents.
            let file_start = phdr.p_offset as usize;
            let file_end = file_start + phdr.p_filesz as usize;
            
            if file_end > self.data.len() {
                return Err(ElfError::InvalidProgramHeader);
            }
            
            let segment_data = &self.data[file_start..file_end];
            
            // Write to memory byte by byte (could be optimized).
            for (i, &byte) in segment_data.iter().enumerate() {
                mem.write_byte(vaddr.wrapping_add(i as u32), byte)?;
            }
            
            // Zero-fill remaining memory (BSS).
            let zero_start = phdr.p_filesz;
            let zero_end = phdr.p_memsz;
            for i in zero_start..zero_end {
                mem.write_byte(vaddr.wrapping_add(i), 0)?;
            }
        }
        
        Ok(())
    }
    
    /// Calculate the highest address used by loaded segments.
    pub fn max_address(&self) -> u32 {
        let mut max = 0u32;
        for phdr in &self.program_headers {
            if phdr.p_type != PT_LOAD {
                continue;
            }
            let vaddr = if self.is_pie {
                self.load_base.wrapping_add(phdr.p_vaddr)
            } else {
                phdr.p_vaddr
            };
            let end = vaddr.wrapping_add(phdr.p_memsz);
            if end > max {
                max = end;
            }
        }
        max
    }
    
    /// Set up CPU state for execution.
    /// 
    /// This initializes:
    /// - PC to entry point
    /// - SP to top of stack
    /// - argc/argv on stack (if provided)
    /// - Thumb mode if entry point bit 0 is set
    pub fn setup_cpu<M: ArmMemory>(
        &self,
        cpu: &mut Armv7Cpu,
        mem: &mut M,
        args: &[&str],
        env: &[&str],
        stack_top: u32,
    ) -> Result<u32, ElfError> {
        let entry = self.entry_point();
        
        // Check if entry point indicates Thumb mode.
        let is_thumb = (entry & 1) != 0;
        let entry_addr = entry & !1;
        
        // Set PC.
        cpu.regs[15] = entry_addr;
        cpu.cpsr.t = is_thumb;
        
        // Build stack with argc, argv, envp (Linux ARM ABI).
        let mut sp = stack_top & !7; // Align to 8 bytes.
        
        // Reserve space for string data.
        let mut strings: Vec<u32> = Vec::new();
        
        // Write environment strings (in reverse order).
        for env_str in env.iter().rev() {
            sp -= (env_str.len() + 1) as u32;
            sp &= !3; // Align to 4 bytes.
            for (i, &byte) in env_str.as_bytes().iter().enumerate() {
                mem.write_byte(sp + i as u32, byte)?;
            }
            mem.write_byte(sp + env_str.len() as u32, 0)?;
            strings.push(sp);
        }
        strings.reverse();
        let env_ptrs: Vec<u32> = strings.drain(..).collect();
        
        // Write argument strings (in reverse order).
        for arg in args.iter().rev() {
            sp -= (arg.len() + 1) as u32;
            sp &= !3; // Align to 4 bytes.
            for (i, &byte) in arg.as_bytes().iter().enumerate() {
                mem.write_byte(sp + i as u32, byte)?;
            }
            mem.write_byte(sp + arg.len() as u32, 0)?;
            strings.push(sp);
        }
        strings.reverse();
        let argv_ptrs: Vec<u32> = strings.drain(..).collect();
        
        // Align stack for ABI.
        sp &= !7;
        
        // Build stack frame:
        // [sp+0]  = argc
        // [sp+4]  = argv[0]
        // ...
        // [sp+4+4*argc] = NULL
        // [sp+...] = envp[0]
        // ...
        // [sp+...] = NULL
        // [sp+...] = auxv (if any, we skip for now)
        
        // Calculate required stack space.
        let argc = args.len() as u32;
        let stack_size = 4 + // argc
            4 * (argc + 1) + // argv + NULL
            4 * (env.len() as u32 + 1); // envp + NULL
        
        sp -= stack_size;
        sp &= !7; // Final alignment.
        
        let mut offset = 0u32;
        
        // Write argc.
        mem.write_word(sp + offset, argc)?;
        offset += 4;
        
        // Write argv pointers.
        for ptr in &argv_ptrs {
            mem.write_word(sp + offset, *ptr)?;
            offset += 4;
        }
        mem.write_word(sp + offset, 0)?; // NULL terminator.
        offset += 4;
        
        // Write envp pointers.
        for ptr in &env_ptrs {
            mem.write_word(sp + offset, *ptr)?;
            offset += 4;
        }
        mem.write_word(sp + offset, 0)?; // NULL terminator.
        
        // Set stack pointer.
        cpu.regs[13] = sp;
        
        // Clear other registers.
        cpu.regs[0] = 0;
        cpu.regs[1] = 0;
        cpu.regs[2] = 0;
        
        Ok(sp)
    }
    
    /// Check if entry point is Thumb mode.
    pub fn is_thumb_entry(&self) -> bool {
        (self.entry_point() & 1) != 0
    }
    
    /// Get interpreter path (for dynamic executables).
    pub fn interpreter(&self) -> Option<&str> {
        for phdr in &self.program_headers {
            if phdr.p_type == PT_INTERP {
                let start = phdr.p_offset as usize;
                let end = start + phdr.p_filesz as usize;
                if end <= self.data.len() {
                    let bytes = &self.data[start..end];
                    // Remove trailing null.
                    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                    return std::str::from_utf8(&bytes[..len]).ok();
                }
            }
        }
        None
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arm::execution::FlatMemory;
    
    // Minimal ARM ELF header for testing.
    fn make_minimal_elf() -> Vec<u8> {
        let mut elf = vec![0u8; 52 + 32]; // Header + 1 program header.
        
        // ELF magic.
        elf[0..4].copy_from_slice(&ELF_MAGIC);
        elf[4] = ELFCLASS32; // 32-bit.
        elf[5] = ELFDATA2LSB; // Little-endian.
        elf[6] = 1; // EV_CURRENT.
        
        // e_type = ET_EXEC.
        elf[16] = ET_EXEC as u8;
        elf[17] = (ET_EXEC >> 8) as u8;
        
        // e_machine = EM_ARM.
        elf[18] = EM_ARM as u8;
        elf[19] = (EM_ARM >> 8) as u8;
        
        // e_version.
        elf[20] = 1;
        
        // e_entry = 0x8000.
        elf[24] = 0x00;
        elf[25] = 0x80;
        elf[26] = 0x00;
        elf[27] = 0x00;
        
        // e_phoff = 52 (right after header).
        elf[28] = 52;
        
        // e_ehsize = 52.
        elf[40] = 52;
        
        // e_phentsize = 32.
        elf[42] = 32;
        
        // e_phnum = 1.
        elf[44] = 1;
        
        // Program header at offset 52.
        let ph_offset = 52;
        
        // p_type = PT_LOAD.
        elf[ph_offset] = PT_LOAD as u8;
        
        // p_offset = 0.
        // (already zero)
        
        // p_vaddr = 0x8000.
        elf[ph_offset + 8] = 0x00;
        elf[ph_offset + 9] = 0x80;
        
        // p_filesz = 84 (header + phdr).
        elf[ph_offset + 16] = 84;
        
        // p_memsz = 84.
        elf[ph_offset + 20] = 84;
        
        // p_flags = PF_R | PF_X.
        elf[ph_offset + 24] = (PF_R | PF_X) as u8;
        
        elf
    }
    
    #[test]
    fn test_parse_minimal_elf() {
        let elf_data = make_minimal_elf();
        let loader = ElfLoader::parse(&elf_data).unwrap();
        
        assert_eq!(loader.header.e_type, ET_EXEC);
        assert_eq!(loader.header.e_machine, EM_ARM);
        assert_eq!(loader.header.e_entry, 0x8000);
        assert_eq!(loader.program_headers.len(), 1);
        assert_eq!(loader.program_headers[0].p_type, PT_LOAD);
        assert!(!loader.is_pie);
    }
    
    #[test]
    fn test_invalid_magic() {
        let mut elf_data = make_minimal_elf();
        elf_data[0] = 0; // Corrupt magic.
        
        let result = ElfLoader::parse(&elf_data);
        assert!(matches!(result, Err(ElfError::InvalidMagic)));
    }
    
    #[test]
    fn test_not_arm() {
        let mut elf_data = make_minimal_elf();
        elf_data[18] = 0; // Clear machine type.
        elf_data[19] = 0;
        
        let result = ElfLoader::parse(&elf_data);
        assert!(matches!(result, Err(ElfError::NotArm)));
    }
    
    #[test]
    fn test_load_segments() {
        let elf_data = make_minimal_elf();
        let loader = ElfLoader::parse(&elf_data).unwrap();
        
        let mut mem = FlatMemory::new(0x10000, 0); // 64KB starting at 0
        loader.load(&mut mem).unwrap();
        
        // Verify magic was loaded at 0x8000.
        assert_eq!(mem.read_byte(0x8000).unwrap(), 0x7f);
        assert_eq!(mem.read_byte(0x8001).unwrap(), b'E');
        assert_eq!(mem.read_byte(0x8002).unwrap(), b'L');
        assert_eq!(mem.read_byte(0x8003).unwrap(), b'F');
    }
    
    #[test]
    fn test_entry_point() {
        let elf_data = make_minimal_elf();
        let loader = ElfLoader::parse(&elf_data).unwrap();
        
        assert_eq!(loader.entry_point(), 0x8000);
        assert!(!loader.is_thumb_entry());
    }
    
    #[test]
    fn test_pie_load_base() {
        let mut elf_data = make_minimal_elf();
        // Change to ET_DYN.
        elf_data[16] = ET_DYN as u8;
        elf_data[17] = (ET_DYN >> 8) as u8;
        
        let mut loader = ElfLoader::parse(&elf_data).unwrap();
        assert!(loader.is_pie);
        
        loader.set_load_base(0x40000000);
        assert_eq!(loader.entry_point(), 0x40008000);
    }
    
    #[test]
    fn test_thumb_entry() {
        let mut elf_data = make_minimal_elf();
        // Set entry to 0x8001 (Thumb mode indicator).
        elf_data[24] = 0x01;
        elf_data[25] = 0x80;
        
        let loader = ElfLoader::parse(&elf_data).unwrap();
        assert!(loader.is_thumb_entry());
    }
    
    #[test]
    fn test_setup_cpu() {
        let elf_data = make_minimal_elf();
        let loader = ElfLoader::parse(&elf_data).unwrap();
        
        let mut cpu = Armv7Cpu::new();
        let mut mem = FlatMemory::new(0x20000, 0); // 128KB starting at 0
        
        loader.load(&mut mem).unwrap();
        let sp = loader.setup_cpu(&mut cpu, &mut mem, &["test_prog"], &["HOME=/home"], 0x10000).unwrap();
        
        // Check PC.
        assert_eq!(cpu.regs[15], 0x8000);
        
        // Check SP is valid and aligned.
        assert!(sp < 0x10000);
        assert_eq!(sp & 7, 0);
        assert_eq!(cpu.regs[13], sp);
        
        // Check argc on stack.
        assert_eq!(mem.read_word(sp).unwrap(), 1); // argc = 1
    }
    
    #[test]
    fn test_max_address() {
        let elf_data = make_minimal_elf();
        let loader = ElfLoader::parse(&elf_data).unwrap();
        
        // Segment ends at 0x8000 + 84.
        assert_eq!(loader.max_address(), 0x8000 + 84);
    }
}
