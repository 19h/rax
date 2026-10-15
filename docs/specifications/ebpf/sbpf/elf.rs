//! This module relocates a BPF ELF

// Note: Typically ELF shared objects are loaded using the program headers and
// not the section headers.  Since we are leveraging the elfkit crate its much
// easier to use the section headers.  There are cases (reduced size, obfuscation)
// where the section headers may be removed from the ELF.  If that happens then
// this loader will need to be re-written to use the program headers instead.

use crate::{
    aligned_memory::{is_memory_aligned, AlignedMemory},
    ebpf::{self, HOST_ALIGN, INSN_SIZE},
    elf_parser::{
        consts::{
            ELFCLASS64, ELFDATA2LSB, ELFOSABI_NONE, EM_BPF, EM_SBPF, ET_DYN, R_X86_64_32,
            R_X86_64_64, R_X86_64_NONE, R_X86_64_RELATIVE,
        },
        types::{Elf64Ehdr, Elf64Phdr, Elf64Shdr, Elf64Word},
        Elf64, ElfParserError,
    },
    error::EbpfError,
    memory_region::MemoryRegion,
    program::{BuiltinProgram, FunctionRegistry, JitProgram, SBPFVersion},
    verifier::Verifier,
    vm::{Config, ContextObject},
};

use byteorder::{ByteOrder, LittleEndian};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    mem::{self, offset_of},
    ops::Range,
    str,
};

#[cfg(not(feature = "shuttle-test"))]
use std::sync::Arc;

#[cfg(feature = "shuttle-test")]
use shuttle::sync::Arc;

/// Error definitions
// Note: `#[repr(u64)]` is used for `Self::discriminant`, but the actual
// memory layout of this enum's variants is not depended on by the VM.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[repr(u64)]
pub enum ElfError {
    /// Failed to parse ELF file
    #[error("Failed to parse ELF file: {0}")]
    FailedToParse(String),
    /// Entrypoint out of bounds
    #[error("Entrypoint out of bounds")]
    EntrypointOutOfBounds,
    /// Invalid entrypoint
    #[error("Invalid entrypoint")]
    InvalidEntrypoint,
    /// Failed to get section
    #[error("Failed to get section {0}")]
    FailedToGetSection(String),
    /// Unresolved symbol
    #[error("Unresolved symbol ({0}) at instruction #{1:?} (ELF file offset {2:#x})")]
    UnresolvedSymbol(String, usize, usize),
    /// Section not found
    #[error("Section not found: {0}")]
    SectionNotFound(String),
    /// Relative jump out of bounds
    #[error("Relative jump out of bounds at instruction #{0}")]
    RelativeJumpOutOfBounds(usize),
    /// Symbol hash collision
    #[error("Symbol hash collision {0:#x}")]
    SymbolHashCollision(u32),
    /// Incompatible ELF: wrong endianess
    #[error("Incompatible ELF: wrong endianess")]
    WrongEndianess,
    /// Incompatible ELF: wrong ABI
    #[error("Incompatible ELF: wrong ABI")]
    WrongAbi,
    /// Incompatible ELF: wrong machine
    #[error("Incompatible ELF: wrong machine")]
    WrongMachine,
    /// Incompatible ELF: wrong class
    #[error("Incompatible ELF: wrong class")]
    WrongClass,
    /// Not one text section
    #[error("Multiple or no text sections, consider removing llc option: -function-sections")]
    NotOneTextSection,
    /// Read-write data not supported
    #[error("Found writable section ({0}) in ELF, read-write data not supported")]
    WritableSectionNotSupported(String),
    /// Relocation failed, no loadable section contains virtual address
    #[error("Relocation failed, no loadable section contains virtual address {0:#x}")]
    AddressOutsideLoadableSection(u64),
    /// Relocation failed, invalid referenced virtual address
    #[error("Relocation failed, invalid referenced virtual address {0:#x}")]
    InvalidVirtualAddress(u64),
    /// Relocation failed, unknown type
    #[error("Relocation failed, unknown type {0:?}")]
    UnknownRelocation(u32),
    /// Failed to read relocation info
    #[error("Failed to read relocation info")]
    FailedToReadRelocationInfo,
    /// Incompatible ELF: wrong type
    #[error("Incompatible ELF: wrong type")]
    WrongType,
    /// Unknown symbol
    #[error("Unknown symbol with index {0}")]
    UnknownSymbol(usize),
    /// Offset or value is out of bounds
    #[error("Offset or value is out of bounds")]
    ValueOutOfBounds,
    /// Detected sbpf_version required by the executable which are not enabled
    #[error("Detected sbpf_version required by the executable which are not enabled")]
    UnsupportedSBPFVersion,
    /// Invalid program header
    #[error("Invalid ELF program header")]
    InvalidProgramHeader,
}

impl ElfError {
    /// Returns the enum discriminant as a `u64`.
    ///
    /// This is sound only because of the `#[repr(u64)]` attribute on the enum.
    pub fn discriminant(&self) -> u64 {
        unsafe { *std::ptr::addr_of!(*self).cast::<u64>() }
    }
}

impl From<ElfParserError> for ElfError {
    fn from(err: ElfParserError) -> Self {
        match err {
            ElfParserError::InvalidSectionHeader
            | ElfParserError::InvalidString
            | ElfParserError::InvalidSize
            | ElfParserError::Overlap
            | ElfParserError::SectionNotInOrder
            | ElfParserError::NoSectionNameStringTable
            | ElfParserError::InvalidDynamicSectionTable
            | ElfParserError::InvalidRelocationTable
            | ElfParserError::InvalidAlignment
            | ElfParserError::NoStringTable
            | ElfParserError::NoDynamicStringTable
            | ElfParserError::InvalidFileHeader
            | ElfParserError::StringTooLong(_, _) => ElfError::FailedToParse(err.to_string()),
            ElfParserError::InvalidProgramHeader => ElfError::InvalidProgramHeader,
            ElfParserError::OutOfBounds => ElfError::ValueOutOfBounds,
        }
    }
}

fn get_section(elf: &Elf64, name: &[u8]) -> Result<Elf64Shdr, ElfError> {
    for section_header in elf.section_header_table() {
        if elf.section_name(section_header.sh_name)? == name {
            return Ok(section_header.clone());
        }
    }

    Err(ElfError::SectionNotFound(
        std::str::from_utf8(name)
            .unwrap_or("UTF-8 error")
            .to_string(),
    ))
}

// For more information on the BPF instruction set:
// https://github.com/iovisor/bpf-docs/blob/master/eBPF.md

// msb                                                        lsb
// +------------------------+----------------+----+----+--------+
// |immediate               |offset          |src |dst |opcode  |
// +------------------------+----------------+----+----+--------+

// From least significant to most significant bit:
//   8 bit opcode
//   4 bit destination register (dst)
//   4 bit source register (src)
//   16 bit offset
//   32 bit immediate (imm)

/// Byte offset of the immediate field in the instruction
const BYTE_OFFSET_IMMEDIATE: usize = 4;
/// Byte length of the immediate field
const BYTE_LENGTH_IMMEDIATE: usize = 4;

/// BPF relocation types.
#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, Copy, Clone)]
enum BpfRelocationType {
    /// No relocation, placeholder
    R_Bpf_None = 0,
    /// R_BPF_64_64 relocation type is used for ld_imm64 instruction.
    /// The actual to-be-relocated data (0 or section offset) is
    /// stored at r_offset + 4 and the read/write data bitsize is 32
    /// (4 bytes). The relocation can be resolved with the symbol
    /// value plus implicit addend.
    R_Bpf_64_64 = 1,
    /// 64 bit relocation of a ldxdw instruction.  The ldxdw
    /// instruction occupies two instruction slots. The 64-bit address
    /// to load from is split into the 32-bit imm field of each
    /// slot. The first slot's pre-relocation imm field contains the
    /// virtual address (typically same as the file offset) of the
    /// location to load. Relocation involves calculating the
    /// post-load 64-bit physical address referenced by the imm field
    /// and writing that physical address back into the imm fields of
    /// the ldxdw instruction.
    R_Bpf_64_Relative = 8,
    /// Relocation of a call instruction.  The existing imm field
    /// contains either an offset of the instruction to jump to (think
    /// local function call) or a special value of "-1".  If -1 the
    /// symbol must be looked up in the symbol table.  The relocation
    /// entry contains the symbol number to call.  In order to support
    /// both local jumps and calling external symbols a 32-bit hash is
    /// computed and stored in the the call instruction's 32-bit imm
    /// field.  The hash is used later to look up the 64-bit address
    /// to jump to.  In the case of a local jump the hash is
    /// calculated using the current program counter and in the case
    /// of a symbol the hash is calculated using the name of the
    /// symbol.
    R_Bpf_64_32 = 10,
}
impl BpfRelocationType {
    fn from_x86_relocation_type(from: u32) -> Option<BpfRelocationType> {
        match from {
            R_X86_64_NONE => Some(BpfRelocationType::R_Bpf_None),
            R_X86_64_64 => Some(BpfRelocationType::R_Bpf_64_64),
            R_X86_64_RELATIVE => Some(BpfRelocationType::R_Bpf_64_Relative),
            R_X86_64_32 => Some(BpfRelocationType::R_Bpf_64_32),
            _ => None,
        }
    }
}

/// ELF section
#[derive(Debug, PartialEq)]
pub enum Section {
    /// Owned section data.
    ///
    /// The first field is virtual address of the section.
    /// The second field is the actual section data.
    Owned(usize, Vec<u8>),
    /// Borrowed section data.
    ///
    /// The first field is virtual address of the section.
    /// The second field can be used to index the input ELF buffer to
    /// retrieve the section data.
    Borrowed(usize, Range<usize>),
}

/// Builds an owned read-only section buffer from the supplied section data.
///
/// Section addresses are rebased relative to `lowest_addr`. Gaps and trailing
/// bytes are zero-filled. Sections are applied in input order, so later
/// sections overwrite earlier bytes when their ranges overlap.
///
/// All section ranges must fit within `buf_len`.
fn build_owned_ro_section<'a>(
    buf_len: usize,
    lowest_addr: usize,
    ro_slices: impl IntoIterator<Item = (usize, &'a [u8])>,
) -> Vec<u8> {
    // Avoid `vec![0; buf_len]` because it uses jemalloc `calloc`/zeroed allocation,
    // which requires zeroed extents and can force fresh/purged pages.
    //
    // Use `Vec::with_capacity` and initialize the buffer incrementally,
    // allowing jemalloc to reuse warm dirty extents.
    let mut ro_section = Vec::<u8>::with_capacity(buf_len);

    for (section_addr, section_data) in ro_slices {
        let buf_offset_start = section_addr.saturating_sub(lowest_addr);
        let buf_offset_end = buf_offset_start.saturating_add(section_data.len());
        debug_assert!(buf_offset_end <= buf_len);

        if ro_section.len() < buf_offset_start {
            ro_section.resize(buf_offset_start, 0);
        }

        let overwrite_dst = &mut ro_section[buf_offset_start..];
        let overwrite_bytes = section_data.len().min(overwrite_dst.len());
        overwrite_dst[..overwrite_bytes].copy_from_slice(&section_data[..overwrite_bytes]);
        ro_section.extend_from_slice(&section_data[overwrite_bytes..]);
    }

    ro_section.resize(buf_len, 0);
    ro_section
}

/// Elf loader/relocator
#[derive(Debug)]
pub struct Executable<C: ContextObject> {
    /// Loaded and executable elf
    elf_bytes: AlignedMemory<{ HOST_ALIGN }>,
    /// Required SBPF capabilities
    sbpf_version: SBPFVersion,
    /// Read-only section
    ro_section: Section,
    /// Text section virtual address
    text_section_vaddr: u64,
    /// Text section range in `elf_bytes`
    text_section_range: Range<usize>,
    /// Address of the entry point
    entry_pc: usize,
    /// Call resolution map (hash, pc, name)
    function_registry: FunctionRegistry<usize>,
    /// Loader built-in program
    loader: Arc<BuiltinProgram<C>>,
    /// Compiled program and argument
    pub(crate) compiled_program: std::sync::Mutex<Option<Arc<JitProgram>>>,
}

impl<C: PartialEq + ContextObject> PartialEq for Executable<C> {
    fn eq(&self, other: &Self) -> bool {
        self.elf_bytes == other.elf_bytes
            && self.sbpf_version == other.sbpf_version
            && self.ro_section == other.ro_section
            && self.text_section_vaddr == other.text_section_vaddr
            && self.text_section_range == other.text_section_range
            && self.entry_pc == other.entry_pc
            && self.function_registry == other.function_registry
            && *self.loader == *other.loader
            && {
                // In order to avoid a deadlock comparing self with self, we gotta make sure
                // that we clone at least one Arc out of the lock before comparing...
                let other = other.get_compiled_program();
                let this = self
                    .compiled_program
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                *this == other
            }
    }
}

impl<C: ContextObject> Executable<C> {
    /// Get the configuration settings
    pub fn get_config(&self) -> &Config {
        self.loader.get_config()
    }

    /// Get the executable sbpf_version
    pub fn get_sbpf_version(&self) -> SBPFVersion {
        self.sbpf_version
    }

    /// Get the .text section virtual address and bytes
    #[allow(clippy::arithmetic_side_effects)]
    pub fn get_text_bytes(&self) -> (u64, &[u8]) {
        let text_len = self.text_section_range.len();
        let text_bytes = match &self.ro_section {
            // Use the owned data (not elf_bytes) to ensure the text section
            // is consistent with the ro_section if the sections overlap.
            Section::Owned(offset, data) => {
                let text_offset = self.text_section_vaddr as usize - offset;
                &data[text_offset..text_offset + text_len]
            }
            Section::Borrowed(_, _) => &self.elf_bytes.as_slice()[self.text_section_range.clone()],
        };

        (self.text_section_vaddr, text_bytes)
    }

    /// Get the concatenated read-only sections (including the text section)
    pub fn get_ro_section(&self) -> &[u8] {
        match &self.ro_section {
            Section::Owned(_offset, data) => data.as_slice(),
            Section::Borrowed(_offset, byte_range) => {
                &self.elf_bytes.as_slice()[byte_range.clone()]
            }
        }
    }

    /// Get a memory region that can be used to access the merged readonly section
    pub fn get_ro_region(&self) -> MemoryRegion {
        get_ro_region(&self.ro_section, self.elf_bytes.as_slice())
    }

    /// Get the entry point offset into the text section
    pub fn get_entrypoint_instruction_offset(&self) -> usize {
        self.entry_pc
    }

    /// Get the text section offset in the ELF file
    #[cfg(feature = "debugger")]
    pub fn get_text_section_offset(&self) -> u64 {
        self.text_section_range.start as u64
    }

    /// Get the text section virtual address (matches what LLDB sees)
    #[cfg(feature = "debugger")]
    pub fn get_text_section_vaddr(&self) -> u64 {
        self.text_section_vaddr
    }

    /// Get the loader built-in program
    pub fn get_loader(&self) -> &Arc<BuiltinProgram<C>> {
        &self.loader
    }

    /// Get the JIT compiled program
    ///
    /// This function will not block the calling thread even if there is a concurrent ongoing call
    /// to [`Self::jit_compile`].
    pub fn get_compiled_program(&self) -> Option<Arc<JitProgram>> {
        let guard = self
            .compiled_program
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(Arc::clone)
    }

    /// Verify the executable
    pub fn verify<V: Verifier>(&self) -> Result<(), EbpfError> {
        <V as Verifier>::verify(
            self.get_text_bytes().1,
            self.get_sbpf_version(),
            self.get_loader().get_function_registry(),
        )?;
        Ok(())
    }

    /// Remove the compiled program.
    ///
    /// Note that the results can be unpredictable in presence of concurrent ongoing calls to
    /// [`Self::jit_compile`]: based on exact execution ordering this function may take out the
    /// previous program (or `None`) that shorly afterwards gets replaced by a compiled program.
    pub fn take_compiled_program(&self) -> Option<Arc<JitProgram>> {
        let mut guard = self
            .compiled_program
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        guard.take()
    }

    /// Get the function registry
    pub fn get_function_registry(&self) -> &FunctionRegistry<usize> {
        &self.function_registry
    }

    /// Create from raw text section bytes (list of instructions)
    pub fn new_from_text_bytes(
        text_bytes: &[u8],
        loader: Arc<BuiltinProgram<C>>,
        sbpf_version: SBPFVersion,
        mut function_registry: FunctionRegistry<usize>,
    ) -> Result<Self, ElfError> {
        let elf_bytes = AlignedMemory::from_slice(text_bytes);
        let entry_pc = if let Some((_name, pc)) = function_registry.lookup_by_name(b"entrypoint") {
            pc
        } else {
            function_registry.register_function_hashed_legacy(
                &loader,
                !sbpf_version.static_syscalls(),
                *b"entrypoint",
                0,
            )?;
            0
        };
        Ok(Self {
            elf_bytes,
            sbpf_version,
            ro_section: Section::Borrowed(
                if sbpf_version.enable_lower_rodata_vaddr() {
                    ebpf::MM_RODATA_START
                } else {
                    ebpf::MM_BYTECODE_START
                } as usize,
                0..text_bytes.len(),
            ),
            text_section_vaddr: ebpf::MM_BYTECODE_START,
            text_section_range: 0..text_bytes.len(),
            entry_pc,
            function_registry,
            loader,
            compiled_program: None.into(),
        })
    }

    /// Fully loads an ELF
    pub fn load(bytes: &[u8], loader: Arc<BuiltinProgram<C>>) -> Result<Self, ElfError> {
        let sbpf_version = get_sbpf_version(bytes)?;
        let config = loader.get_config();
        if !config.enabled_sbpf_versions.contains(&sbpf_version) {
            return Err(ElfError::UnsupportedSBPFVersion);
        }

        let mut executable = if sbpf_version.enable_stricter_elf_headers() {
            Self::load_with_strict_parser(bytes, loader)?
        } else {
            Self::load_with_lenient_parser(bytes, loader)?
        };
        executable.sbpf_version = sbpf_version;
        Ok(executable)
    }

    /// Loads an ELF without relocation
    pub fn load_with_strict_parser(
        bytes: &[u8],
        loader: Arc<BuiltinProgram<C>>,
    ) -> Result<Self, ElfParserError> {
        use crate::elf_parser::consts::{ELFMAG, EV_CURRENT, PF_R, PF_X, PT_LOAD};

        let aligned_memory = AlignedMemory::<{ HOST_ALIGN }>::from_slice(bytes);
        let elf_bytes = aligned_memory.as_slice();

        let (file_header_range, file_header) = Elf64::parse_file_header(elf_bytes)?;
        let program_header_table_range = mem::size_of::<Elf64Ehdr>()
            ..mem::size_of::<Elf64Phdr>()
                .saturating_mul(file_header.e_phnum as usize)
                .saturating_add(mem::size_of::<Elf64Ehdr>());
        if file_header.e_ident.ei_mag != ELFMAG
            || file_header.e_ident.ei_class != ELFCLASS64
            || file_header.e_ident.ei_data != ELFDATA2LSB
            || file_header.e_ident.ei_version != EV_CURRENT as u8
            || file_header.e_ident.ei_osabi != ELFOSABI_NONE
            || file_header.e_ident.ei_abiversion != 0x00
            || file_header.e_ident.ei_pad != [0x00; 7]
            // file_header.e_type
            || file_header.e_machine != EM_BPF
            || file_header.e_version != EV_CURRENT
            // file_header.e_entry
            || file_header.e_phoff != mem::size_of::<Elf64Ehdr>() as u64
            // file_header.e_shoff
            // file_header.e_flags
            || file_header.e_ehsize != mem::size_of::<Elf64Ehdr>() as u16
            || file_header.e_phentsize != mem::size_of::<Elf64Phdr>() as u16
            || file_header.e_phnum == 0
            || program_header_table_range.end > elf_bytes.len()
        // file_header.e_shentsize
        // file_header.e_shnum
        // file_header.e_shstrndx
        {
            return Err(ElfParserError::InvalidFileHeader);
        }

        const EXPECTED_PROGRAM_HEADERS: [(u32, u64); 2] = [
            (PF_R, ebpf::MM_RODATA_START),   // read only data
            (PF_X, ebpf::MM_BYTECODE_START), // byte code
        ];
        let program_header_table =
            Elf64::slice_from_bytes::<Elf64Phdr>(elf_bytes, program_header_table_range.clone())?;
        let mut expected_program_headers = EXPECTED_PROGRAM_HEADERS.iter();
        let skip_rodata_program_header =
            program_header_table[0].p_flags != EXPECTED_PROGRAM_HEADERS[0].0;
        if skip_rodata_program_header {
            // If the first program header is not marked as PF_R (readonly),
            // then expect to start at the second program header instead.
            expected_program_headers.next();
        } else if file_header.e_phnum < 2 {
            return Err(ElfParserError::InvalidFileHeader);
        }
        let mut expected_offset = program_header_table_range.end as u64;
        for (program_header, (p_flags, p_vaddr)) in
            program_header_table.iter().zip(expected_program_headers)
        {
            if program_header.p_type != PT_LOAD
                || program_header.p_flags != *p_flags
                || program_header.p_offset != expected_offset
                || program_header.p_offset >= elf_bytes.len() as u64
                || program_header.p_offset.checked_rem(ebpf::INSN_SIZE as u64) != Some(0)
                || program_header.p_vaddr != *p_vaddr
                || program_header.p_paddr != *p_vaddr
                || program_header.p_filesz != program_header.p_memsz
                || program_header.p_filesz
                    > (elf_bytes.len() as u64).saturating_sub(program_header.p_offset)
                || program_header.p_filesz.checked_rem(ebpf::INSN_SIZE as u64) != Some(0)
                || program_header.p_memsz >= ebpf::MM_REGION_SIZE
            {
                return Err(ElfParserError::InvalidProgramHeader);
            }
            expected_offset = expected_offset.saturating_add(program_header.p_filesz);
        }

        let (ro_section_range, bytecode_header) = if skip_rodata_program_header {
            (
                program_header_table_range.end..program_header_table_range.end,
                &program_header_table[0],
            )
        } else {
            (
                program_header_table[0].file_range().unwrap_or_default(),
                &program_header_table[1],
            )
        };
        let ro_section = Section::Borrowed(ebpf::MM_RODATA_START as usize, ro_section_range);
        let text_section_vaddr = bytecode_header.p_vaddr;
        let text_section_range = bytecode_header.file_range().unwrap_or_default();

        if !bytecode_header.vm_range().contains(
            &file_header
                .e_entry
                .saturating_add(ebpf::INSN_SIZE as u64)
                .saturating_sub(1),
        ) || file_header.e_entry.checked_rem(ebpf::INSN_SIZE as u64) != Some(0)
        {
            return Err(ElfParserError::InvalidFileHeader);
        }
        let entry_pc = file_header
            .e_entry
            .saturating_sub(bytecode_header.p_vaddr)
            .checked_div(ebpf::INSN_SIZE as u64)
            .unwrap_or_default() as usize;

        let config = loader.get_config();
        // Labels are debug information, so anything that goes wrong while
        // collecting them leaves the executable without labels instead of
        // failing the load.
        let function_registry = if config.enable_symbol_and_section_labels {
            Self::parse_debug_labels(
                elf_bytes,
                file_header,
                file_header_range,
                program_header_table_range,
                bytecode_header,
            )
            .unwrap_or_default()
        } else {
            FunctionRegistry::<usize>::default()
        };

        Ok(Self {
            elf_bytes: aligned_memory,
            sbpf_version: SBPFVersion::Reserved, // Is set in Self::load()
            ro_section,
            text_section_vaddr,
            text_section_range,
            entry_pc,
            function_registry,
            loader,
            compiled_program: None.into(),
        })
    }

    /// Collects the function labels of a strictly parsed ELF from its static
    /// symbol table
    ///
    /// Callers treat a failure here as "this ELF carries no labels".
    fn parse_debug_labels(
        elf_bytes: &[u8],
        file_header: &Elf64Ehdr,
        file_header_range: std::ops::Range<usize>,
        program_header_table_range: std::ops::Range<usize>,
        bytecode_header: &Elf64Phdr,
    ) -> Result<FunctionRegistry<usize>, ElfParserError> {
        use crate::elf_parser::{
            consts::{SHN_UNDEF, STT_FUNC},
            types::Elf64Sym,
            DEBUG_SYMBOL_NAME_LENGTH_MAXIMUM, SECTION_NAME_LENGTH_MAXIMUM,
        };

        let mut function_registry = FunctionRegistry::<usize>::default();
        let (_section_header_table_range, section_header_table) =
            Elf64::parse_section_header_table(
                elf_bytes,
                file_header_range,
                file_header,
                program_header_table_range,
            )?;
        if file_header.e_shstrndx == SHN_UNDEF {
            return Ok(function_registry);
        }
        let section_names_section_header = section_header_table
            .get(file_header.e_shstrndx as usize)
            .ok_or(ElfParserError::OutOfBounds)?;
        let mut symbol_names_section_header = None;
        let mut symbol_table_section_header = None;
        for section_header in section_header_table.iter() {
            let Ok(section_name) = Elf64::get_string_in_section(
                elf_bytes,
                section_names_section_header,
                section_header.sh_name,
                SECTION_NAME_LENGTH_MAXIMUM,
            ) else {
                continue;
            };
            if section_name == b".strtab" {
                symbol_names_section_header = Some(section_header);
            }
            if section_name == b".symtab" {
                symbol_table_section_header = Some(section_header);
            }
        }
        // Stripping removes the symbol table, and can leave an empty string
        // table behind, so anything but a complete pair means no labels.
        let (Some(symbol_names_section_header), Some(symbol_table_section_header)) =
            (symbol_names_section_header, symbol_table_section_header)
        else {
            return Ok(function_registry);
        };
        let symbol_table: &[Elf64Sym] =
            Elf64::slice_from_section_header(elf_bytes, symbol_table_section_header)?;
        for symbol in symbol_table {
            if symbol.st_info & STT_FUNC == 0 {
                continue;
            }
            let target_pc = symbol
                .st_value
                .saturating_sub(bytecode_header.p_vaddr)
                .checked_div(ebpf::INSN_SIZE as u64)
                .unwrap_or_default() as usize;
            let Ok(name) = Elf64::get_string_in_section(
                elf_bytes,
                symbol_names_section_header,
                symbol.st_name as Elf64Word,
                DEBUG_SYMBOL_NAME_LENGTH_MAXIMUM,
            ) else {
                // A name we cannot read only costs this one label.
                continue;
            };
            let _ = function_registry.register_function(target_pc as u32, name, target_pc);
        }
        Ok(function_registry)
    }

    /// Loads an ELF with relocation
    fn load_with_lenient_parser(
        bytes: &[u8],
        loader: Arc<BuiltinProgram<C>>,
    ) -> Result<Self, ElfError> {
        // We always need one memory copy to take ownership and for relocations
        let aligned_memory = AlignedMemory::<{ HOST_ALIGN }>::from_slice(bytes);
        let (mut elf_bytes, unrelocated_elf_bytes) =
            if is_memory_aligned(bytes.as_ptr() as usize, HOST_ALIGN) {
                (aligned_memory, bytes)
            } else {
                // We might need another memory copy to ensure alignment
                (aligned_memory.clone(), aligned_memory.as_slice())
            };
        let elf = Elf64::parse(unrelocated_elf_bytes)?;

        let config = loader.get_config();
        let header = elf.file_header();

        Self::validate(&elf, elf_bytes.as_slice())?;

        // calculate the text section info
        let text_section = get_section(&elf, b".text")?;
        let text_section_vaddr = text_section.sh_addr.saturating_add(ebpf::MM_REGION_SIZE);
        if (config.reject_broken_elfs && text_section.sh_addr != text_section.sh_offset)
            || text_section_vaddr >= ebpf::MM_STACK_START
        {
            return Err(ElfError::ValueOutOfBounds);
        }

        // relocate symbols
        let mut function_registry = FunctionRegistry::default();
        Self::relocate(
            &mut function_registry,
            &loader,
            &elf,
            elf_bytes.as_slice_mut(),
        )?;

        // calculate entrypoint offset into the text section
        let offset = header.e_entry.saturating_sub(text_section.sh_addr);
        if offset.checked_rem(ebpf::INSN_SIZE as u64) != Some(0) {
            return Err(ElfError::InvalidEntrypoint);
        }
        let entry_pc = if let Some(entry_pc) = (offset as usize).checked_div(ebpf::INSN_SIZE) {
            function_registry.unregister_function(ebpf::hash_symbol_name(b"entrypoint"));
            function_registry.register_function_hashed_legacy(
                &loader,
                true,
                *b"entrypoint",
                entry_pc,
            )?;
            entry_pc
        } else {
            return Err(ElfError::InvalidEntrypoint);
        };

        let ro_section = Self::parse_ro_sections(
            config,
            elf.section_header_table()
                .iter()
                .map(|s| (elf.section_name(s.sh_name).ok(), s)),
            elf_bytes.as_slice(),
        )?;
        let ro_section_vaddr = match &ro_section {
            Section::Owned(offset, _data) => *offset,
            Section::Borrowed(offset, _byte_range) => *offset,
        } as u64;
        if config.optimize_rodata {
            let ro_section_index = ro_section_vaddr
                .checked_shr(ebpf::VIRTUAL_ADDRESS_BITS as u32)
                .unwrap_or(0);
            if ro_section_index != 1 {
                return Err(ElfError::ValueOutOfBounds);
            }
        } else {
            debug_assert_eq!(ro_section_vaddr, ebpf::MM_REGION_SIZE);
        }

        Ok(Self {
            elf_bytes,
            sbpf_version: SBPFVersion::Reserved, // Is set in Self::load()
            ro_section,
            text_section_vaddr,
            text_section_range: text_section.file_range().unwrap_or_default(),
            entry_pc,
            function_registry,
            loader,
            compiled_program: None.into(),
        })
    }

    /// Calculate the total memory size of the executable
    #[rustfmt::skip]
    #[allow(clippy::size_of_ref)]
    pub fn mem_size(&self) -> usize {
        let mut total = mem::size_of::<Self>();
        total = total
            // elf bytes
            .saturating_add(self.elf_bytes.mem_size())
            // ro section
            .saturating_add(match &self.ro_section {
                Section::Owned(_, data) => data.capacity(),
                Section::Borrowed(_, _) => 0,
            })
            // bpf functions
            .saturating_add(self.function_registry.mem_size());

        // compiled programs
        let prog = self.compiled_program.lock().unwrap_or_else(|e| e.into_inner());
        total = total.saturating_add(prog.as_ref().map_or(0, |program| program.mem_size()));

        total
    }

    // Functions exposed for tests

    /// Validates the ELF
    pub fn validate(elf: &Elf64, elf_bytes: &[u8]) -> Result<(), ElfError> {
        let header = elf.file_header();
        if header.e_ident.ei_class != ELFCLASS64 {
            return Err(ElfError::WrongClass);
        }
        if header.e_ident.ei_data != ELFDATA2LSB {
            return Err(ElfError::WrongEndianess);
        }
        if header.e_ident.ei_osabi != ELFOSABI_NONE {
            return Err(ElfError::WrongAbi);
        }
        if header.e_machine != EM_BPF && header.e_machine != EM_SBPF {
            return Err(ElfError::WrongMachine);
        }
        if header.e_type != ET_DYN {
            return Err(ElfError::WrongType);
        }

        let num_text_sections =
            elf.section_header_table()
                .iter()
                .fold(0, |count: usize, section_header| {
                    if let Ok(this_name) = elf.section_name(section_header.sh_name) {
                        if this_name == b".text" {
                            return count.saturating_add(1);
                        }
                    }
                    count
                });
        if 1 != num_text_sections {
            return Err(ElfError::NotOneTextSection);
        }

        for section_header in elf.section_header_table().iter() {
            if let Ok(name) = elf.section_name(section_header.sh_name) {
                if name.starts_with(b".bss")
                    || (section_header.is_writable()
                        && (name.starts_with(b".data") && !name.starts_with(b".data.rel")))
                {
                    return Err(ElfError::WritableSectionNotSupported(
                        String::from_utf8_lossy(name).to_string(),
                    ));
                }
            }
        }

        for section_header in elf.section_header_table().iter() {
            let start = section_header.sh_offset as usize;
            let end = section_header
                .sh_offset
                .checked_add(section_header.sh_size)
                .ok_or(ElfError::ValueOutOfBounds)? as usize;
            let _ = elf_bytes
                .get(start..end)
                .ok_or(ElfError::ValueOutOfBounds)?;
        }
        let text_section = get_section(elf, b".text")?;
        if !text_section.vm_range().contains(&header.e_entry) {
            return Err(ElfError::EntrypointOutOfBounds);
        }

        Ok(())
    }

    /// Parses and concatenates the readonly data sections
    pub fn parse_ro_sections<'a, S: IntoIterator<Item = (Option<&'a [u8]>, &'a Elf64Shdr)>>(
        config: &Config,
        sections: S,
        elf_bytes: &[u8],
    ) -> Result<Section, ElfError> {
        // the lowest section address
        let mut lowest_addr = usize::MAX;
        // the highest section address
        let mut highest_addr = 0;
        // the aggregated section length, not including gaps between sections
        let mut ro_fill_length = 0usize;
        let mut invalid_offsets = false;

        // keep track of where ro sections are so we can tell whether they're
        // contiguous
        let mut first_ro_section = 0;
        let mut last_ro_section = 0;
        let mut n_ro_sections = 0usize;

        let mut ro_slices = vec![];
        for (i, (name, section_header)) in sections.into_iter().enumerate() {
            match name {
                Some(name)
                    if name == b".text"
                        || name == b".rodata"
                        || name == b".data.rel.ro"
                        || name == b".eh_frame" => {}
                _ => continue,
            }

            if n_ro_sections == 0 {
                first_ro_section = i;
            }
            last_ro_section = i;
            n_ro_sections = n_ro_sections.saturating_add(1);

            let section_addr = section_header.sh_addr;

            // sh_offset handling:
            // section_addr must match sh_offset
            if !invalid_offsets && section_addr != section_header.sh_offset {
                invalid_offsets = true;
            }

            let vaddr_end = section_addr.saturating_add(ebpf::MM_REGION_SIZE);
            if (config.reject_broken_elfs && invalid_offsets) || vaddr_end > ebpf::MM_STACK_START {
                return Err(ElfError::ValueOutOfBounds);
            }

            let section_data = elf_bytes
                .get(section_header.file_range().unwrap_or_default())
                .ok_or(ElfError::ValueOutOfBounds)?;

            let section_addr = section_addr as usize;
            lowest_addr = lowest_addr.min(section_addr);
            highest_addr = highest_addr.max(section_addr.saturating_add(section_data.len()));
            ro_fill_length = ro_fill_length.saturating_add(section_data.len());

            // Empty sections still shape the mappable bounds above, but they
            // contain no data to copy.
            if !section_data.is_empty() {
                ro_slices.push((section_addr, section_data));
            }
        }

        if config.reject_broken_elfs && lowest_addr.saturating_add(ro_fill_length) > highest_addr {
            return Err(ElfError::ValueOutOfBounds);
        }

        let can_borrow = !invalid_offsets
            && last_ro_section
                .saturating_add(1)
                .saturating_sub(first_ro_section)
                == n_ro_sections;
        let ro_section = if config.optimize_rodata && can_borrow {
            // Read only sections are grouped together with no intermixed non-ro
            // sections. We can borrow.

            let addr_offset = if lowest_addr >= ebpf::MM_REGION_SIZE as usize {
                // The first field of Section::Borrowed is an offset from
                // ebpf::MM_REGION_SIZE * 1 so if the linker has already put the
                // sections within ebpf::MM_REGION_SIZE * 1, we need to subtract
                // it now.
                lowest_addr
            } else {
                lowest_addr.saturating_add(ebpf::MM_REGION_SIZE as usize)
            };

            Section::Borrowed(addr_offset, lowest_addr..highest_addr)
        } else {
            // Read only and other non-ro sections are mixed. Zero the non-ro
            // sections and and copy the ro ones at their intended offsets.

            if config.optimize_rodata {
                // The rodata region starts at MM_REGION_SIZE * 1 + offset,
                // [MM_REGION_SIZE * 1, MM_REGION_SIZE * 1 + offset) is not
                // mappable. We only need to allocate highest_addr - lowest_addr
                // bytes.
                highest_addr = highest_addr.saturating_sub(lowest_addr);
            } else {
                // For backwards compatibility, the whole [MM_REGION_SIZE * 1,
                // MM_REGION_SIZE * 1 + highest_addr) range is mappable. We need
                // to allocate the whole address range.
                lowest_addr = 0;
            };

            let buf_len = highest_addr;
            if buf_len > elf_bytes.len() {
                return Err(ElfError::ValueOutOfBounds);
            }

            let ro_section = build_owned_ro_section(buf_len, lowest_addr, ro_slices);

            let addr_offset = if lowest_addr >= ebpf::MM_REGION_SIZE as usize {
                lowest_addr
            } else {
                lowest_addr.saturating_add(ebpf::MM_REGION_SIZE as usize)
            };
            Section::Owned(addr_offset, ro_section)
        };

        Ok(ro_section)
    }

    /// Relocates the ELF in-place
    fn relocate(
        function_registry: &mut FunctionRegistry<usize>,
        loader: &BuiltinProgram<C>,
        elf: &Elf64,
        elf_bytes: &mut [u8],
    ) -> Result<(), ElfError> {
        let mut syscall_cache = BTreeMap::new();
        let text_section = get_section(elf, b".text")?;

        // Fixup all program counter relative call instructions
        let config = loader.get_config();
        let text_bytes = elf_bytes
            .get_mut(text_section.file_range().unwrap_or_default())
            .ok_or(ElfError::ValueOutOfBounds)?;
        let instruction_count = text_bytes
            .len()
            .checked_div(ebpf::INSN_SIZE)
            .ok_or(ElfError::ValueOutOfBounds)?;
        for i in 0..instruction_count {
            let insn = ebpf::get_insn(text_bytes, i);
            if insn.opc == ebpf::CALL_IMM && insn.imm != -1 {
                let target_pc = (i as isize)
                    .saturating_add(1)
                    .saturating_add(insn.imm as isize);
                if target_pc < 0 || target_pc >= instruction_count as isize {
                    return Err(ElfError::RelativeJumpOutOfBounds(i));
                }
                let name = if config.enable_symbol_and_section_labels {
                    format!("function_{target_pc}")
                } else {
                    String::default()
                };
                let key = function_registry.register_function_hashed_legacy(
                    loader,
                    true,
                    name.as_bytes(),
                    target_pc as usize,
                )?;
                let offset = i.saturating_mul(ebpf::INSN_SIZE).saturating_add(4);
                let checked_slice = text_bytes
                    .get_mut(offset..offset.saturating_add(4))
                    .ok_or(ElfError::ValueOutOfBounds)?;
                LittleEndian::write_u32(checked_slice, key);
            }
        }

        // Fixup all the relocations in the relocation section if exists
        for relocation in elf.dynamic_relocations_table().unwrap_or_default().iter() {
            let r_offset = relocation.r_offset as usize;

            match BpfRelocationType::from_x86_relocation_type(relocation.r_type()) {
                Some(BpfRelocationType::R_Bpf_64_64) => {
                    // Offset of the immediate field
                    let imm_offset = r_offset.saturating_add(BYTE_OFFSET_IMMEDIATE);

                    // Read the instruction's immediate field which contains virtual
                    // address to convert to physical
                    let checked_slice = elf_bytes
                        .get(imm_offset..imm_offset.saturating_add(BYTE_LENGTH_IMMEDIATE))
                        .ok_or(ElfError::ValueOutOfBounds)?;
                    let refd_addr = LittleEndian::read_u32(checked_slice) as u64;

                    let symbol = elf
                        .dynamic_symbol_table()
                        .and_then(|table| table.get(relocation.r_sym() as usize).cloned())
                        .ok_or_else(|| ElfError::UnknownSymbol(relocation.r_sym() as usize))?;

                    // The relocated address is relative to the address of the
                    // symbol at index `r_sym`
                    let mut addr = symbol.st_value.saturating_add(refd_addr);

                    // The "physical address" from the VM's perspective is rooted
                    // at ebpf::MM_REGION_SIZE * 1. If the linker hasn't already put
                    // the symbol within ebpf::MM_REGION_SIZE * 1, we need to do so
                    // now.
                    if addr < ebpf::MM_REGION_SIZE {
                        addr = ebpf::MM_REGION_SIZE.saturating_add(addr);
                    }

                    let imm_low_offset = imm_offset;
                    let imm_high_offset = imm_low_offset.saturating_add(INSN_SIZE);

                    // Write the low side of the relocate address
                    let imm_slice = elf_bytes
                        .get_mut(
                            imm_low_offset..imm_low_offset.saturating_add(BYTE_LENGTH_IMMEDIATE),
                        )
                        .ok_or(ElfError::ValueOutOfBounds)?;
                    LittleEndian::write_u32(imm_slice, (addr & 0xFFFFFFFF) as u32);

                    // Write the high side of the relocate address
                    let imm_slice = elf_bytes
                        .get_mut(
                            imm_high_offset..imm_high_offset.saturating_add(BYTE_LENGTH_IMMEDIATE),
                        )
                        .ok_or(ElfError::ValueOutOfBounds)?;
                    LittleEndian::write_u32(
                        imm_slice,
                        addr.checked_shr(32).unwrap_or_default() as u32,
                    );
                }
                Some(BpfRelocationType::R_Bpf_64_Relative) => {
                    // Relocation between different sections, where the target
                    // memory is not associated to a symbol (eg some compiler
                    // generated rodata that doesn't have an explicit symbol).

                    // Offset of the immediate field
                    let imm_offset = r_offset.saturating_add(BYTE_OFFSET_IMMEDIATE);

                    if text_section
                        .file_range()
                        .unwrap_or_default()
                        .contains(&r_offset)
                    {
                        // We're relocating a lddw instruction, which spans two
                        // instruction slots. The address to be relocated is
                        // split in two halves in the two imms of the
                        // instruction slots.
                        let imm_low_offset = imm_offset;
                        let imm_high_offset = r_offset
                            .saturating_add(INSN_SIZE)
                            .saturating_add(BYTE_OFFSET_IMMEDIATE);

                        // Read the low side of the address
                        let imm_slice = elf_bytes
                            .get(
                                imm_low_offset
                                    ..imm_low_offset.saturating_add(BYTE_LENGTH_IMMEDIATE),
                            )
                            .ok_or(ElfError::ValueOutOfBounds)?;
                        let va_low = LittleEndian::read_u32(imm_slice) as u64;

                        // Read the high side of the address
                        let imm_slice = elf_bytes
                            .get(
                                imm_high_offset
                                    ..imm_high_offset.saturating_add(BYTE_LENGTH_IMMEDIATE),
                            )
                            .ok_or(ElfError::ValueOutOfBounds)?;
                        let va_high = LittleEndian::read_u32(imm_slice) as u64;

                        // Put the address back together
                        let mut refd_addr = va_high.checked_shl(32).unwrap_or_default() | va_low;

                        if refd_addr == 0 {
                            return Err(ElfError::InvalidVirtualAddress(refd_addr));
                        }

                        if refd_addr < ebpf::MM_REGION_SIZE {
                            // The linker hasn't already placed rodata within
                            // ebpf::MM_REGION_SIZE * 1, so we do so now
                            refd_addr = ebpf::MM_REGION_SIZE.saturating_add(refd_addr);
                        }

                        // Write back the low half
                        let imm_slice = elf_bytes
                            .get_mut(
                                imm_low_offset
                                    ..imm_low_offset.saturating_add(BYTE_LENGTH_IMMEDIATE),
                            )
                            .ok_or(ElfError::ValueOutOfBounds)?;
                        LittleEndian::write_u32(imm_slice, (refd_addr & 0xFFFFFFFF) as u32);

                        // Write back the high half
                        let imm_slice = elf_bytes
                            .get_mut(
                                imm_high_offset
                                    ..imm_high_offset.saturating_add(BYTE_LENGTH_IMMEDIATE),
                            )
                            .ok_or(ElfError::ValueOutOfBounds)?;
                        LittleEndian::write_u32(
                            imm_slice,
                            refd_addr.checked_shr(32).unwrap_or_default() as u32,
                        );
                    } else {
                        // There used to be a bug in toolchains before
                        // https://github.com/solana-labs/llvm-project/pull/35 where for 64 bit
                        // relocations we were encoding only the low 32 bits, shifted 32 bits to
                        // the left. Our relocation code used to be compatible with that, so we
                        // need to keep supporting this case for backwards compatibility.
                        let addr_slice = elf_bytes
                            .get(imm_offset..imm_offset.saturating_add(BYTE_LENGTH_IMMEDIATE))
                            .ok_or(ElfError::ValueOutOfBounds)?;
                        let mut refd_addr = LittleEndian::read_u32(addr_slice) as u64;
                        refd_addr = ebpf::MM_REGION_SIZE.saturating_add(refd_addr);

                        let addr_slice = elf_bytes
                            .get_mut(r_offset..r_offset.saturating_add(mem::size_of::<u64>()))
                            .ok_or(ElfError::ValueOutOfBounds)?;
                        LittleEndian::write_u64(addr_slice, refd_addr);
                    }
                }
                Some(BpfRelocationType::R_Bpf_64_32) => {
                    // The .text section has an unresolved call to symbol instruction
                    // Hash the symbol name and stick it into the call instruction's imm
                    // field.  Later that hash will be used to look up the function location.

                    // Offset of the immediate field
                    let imm_offset = r_offset.saturating_add(BYTE_OFFSET_IMMEDIATE);

                    let symbol = elf
                        .dynamic_symbol_table()
                        .and_then(|table| table.get(relocation.r_sym() as usize).cloned())
                        .ok_or_else(|| ElfError::UnknownSymbol(relocation.r_sym() as usize))?;

                    let name = elf
                        .dynamic_symbol_name(symbol.st_name as Elf64Word)
                        .map_err(|_| ElfError::UnknownSymbol(symbol.st_name as usize))?;

                    // If the symbol is defined, this is a bpf-to-bpf call
                    let key = if symbol.is_function() && symbol.st_value != 0 {
                        if !text_section.vm_range().contains(&symbol.st_value) {
                            return Err(ElfError::ValueOutOfBounds);
                        }
                        let target_pc = (symbol.st_value.saturating_sub(text_section.sh_addr)
                            as usize)
                            .checked_div(ebpf::INSN_SIZE)
                            .unwrap_or_default();
                        function_registry
                            .register_function_hashed_legacy(loader, true, name, target_pc)?
                    } else {
                        // Else it's a syscall
                        let hash = *syscall_cache
                            .entry(symbol.st_name)
                            .or_insert_with(|| ebpf::hash_symbol_name(name));
                        if config.reject_broken_elfs
                            && loader.get_function_registry().lookup_by_key(hash).is_none()
                        {
                            return Err(ElfError::UnresolvedSymbol(
                                String::from_utf8_lossy(name).to_string(),
                                r_offset.checked_div(ebpf::INSN_SIZE).unwrap_or(0),
                                r_offset,
                            ));
                        }
                        hash
                    };

                    let checked_slice = elf_bytes
                        .get_mut(imm_offset..imm_offset.saturating_add(BYTE_LENGTH_IMMEDIATE))
                        .ok_or(ElfError::ValueOutOfBounds)?;
                    LittleEndian::write_u32(checked_slice, key);
                }
                _ => return Err(ElfError::UnknownRelocation(relocation.r_type())),
            }
        }

        if config.enable_symbol_and_section_labels {
            // Register all known function names from the symbol table
            for symbol in elf.symbol_table().ok().flatten().unwrap_or_default().iter() {
                if symbol.st_info & 0xEF != 0x02 {
                    continue;
                }
                if !text_section.vm_range().contains(&symbol.st_value) {
                    return Err(ElfError::ValueOutOfBounds);
                }
                let target_pc = (symbol.st_value.saturating_sub(text_section.sh_addr) as usize)
                    .checked_div(ebpf::INSN_SIZE)
                    .unwrap_or_default();
                let Ok(name) = elf.symbol_name(symbol.st_name as Elf64Word) else {
                    // Labels are debug information: a name we cannot read only
                    // costs this one label.
                    continue;
                };
                function_registry.register_function_hashed_legacy(loader, true, name, target_pc)?;
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn dump_data(name: &str, prog: &[u8]) {
        let mut eight_bytes: Vec<u8> = Vec::new();
        println!("{name}");
        for i in prog.iter() {
            if eight_bytes.len() >= 7 {
                println!("{eight_bytes:02X?}");
                eight_bytes.clear();
            } else {
                eight_bytes.push(*i);
            }
        }
    }
}

/// Reads the SBPF version from an ELF's `e_flags` header field.
pub fn get_sbpf_version(elf_bytes: &[u8]) -> Result<SBPFVersion, ElfParserError> {
    const E_FLAGS_OFFSET: usize = offset_of!(Elf64Ehdr, e_flags);
    const E_FLAGS_END: usize = E_FLAGS_OFFSET + mem::size_of::<u32>();
    let e_flags = LittleEndian::read_u32(
        elf_bytes
            .get(E_FLAGS_OFFSET..E_FLAGS_END)
            .ok_or(ElfParserError::OutOfBounds)?,
    );
    Ok(match e_flags {
        0 => SBPFVersion::V0,
        1 => SBPFVersion::V1,
        2 => SBPFVersion::V2,
        3 => SBPFVersion::V3,
        4 => SBPFVersion::V4,
        _ => SBPFVersion::Reserved,
    })
}

/// Creates a [MemoryRegion] for the given [Section]
pub fn get_ro_region(ro_section: &Section, elf: &[u8]) -> MemoryRegion {
    let (offset, ro_data) = match ro_section {
        Section::Owned(offset, data) => (*offset, data.as_slice()),
        Section::Borrowed(offset, byte_range) => (*offset, &elf[byte_range.clone()]),
    };

    // If offset > 0, the region will start at ebpf::MM_REGION_SIZE * 1 + the offset of
    // the first read only byte. [ebpf::MM_REGION_SIZE * 1, ebpf::MM_REGION_SIZE * 1 + offset)
    // will be unmappable, see MemoryRegion::vm_to_host.
    MemoryRegion::new(&raw const *ro_data, offset as u64)
}

#[cfg(test)]
mod tests {
    use super::build_owned_ro_section;

    struct TestCase {
        name: &'static str,
        buf_len: usize,
        lowest_addr: usize,
        ro_slices: &'static [(usize, &'static [u8])],
        expected: &'static [u8],
    }

    #[test]
    fn test_build_owned_ro_section() {
        let test_cases = [
            TestCase {
                name: "empty",
                buf_len: 4,
                lowest_addr: 0,
                ro_slices: &[],
                expected: &[0; 4],
            },
            TestCase {
                name: "ordered contiguous",
                buf_len: 6,
                lowest_addr: 100,
                ro_slices: &[(100, &[1, 2]), (102, &[3, 4]), (104, &[5, 6])],
                expected: &[1, 2, 3, 4, 5, 6],
            },
            TestCase {
                name: "ordered with gaps",
                buf_len: 10,
                lowest_addr: 0,
                ro_slices: &[(2, &[1, 2]), (6, &[3, 4])],
                expected: &[0, 0, 1, 2, 0, 0, 3, 4, 0, 0],
            },
            TestCase {
                name: "contained overlap",
                buf_len: 6,
                lowest_addr: 0,
                ro_slices: &[(0, &[1, 2, 3, 4, 5]), (1, &[9, 8])],
                expected: &[1, 9, 8, 4, 5, 0],
            },
            TestCase {
                name: "extending overlap",
                buf_len: 6,
                lowest_addr: 0,
                ro_slices: &[(0, &[1, 2, 3]), (2, &[9, 8, 7])],
                expected: &[1, 2, 9, 8, 7, 0],
            },
            TestCase {
                name: "reversed disjoint",
                buf_len: 6,
                lowest_addr: 0,
                ro_slices: &[(4, &[5, 6]), (2, &[3, 4]), (0, &[1, 2])],
                expected: &[1, 2, 3, 4, 5, 6],
            },
            TestCase {
                name: "overlaps preserve input order",
                buf_len: 6,
                lowest_addr: 0,
                ro_slices: &[(2, &[3, 3, 3, 3]), (0, &[1, 1, 1, 1]), (3, &[9, 9, 9])],
                expected: &[1, 1, 1, 9, 9, 9],
            },
        ];

        for TestCase {
            name,
            buf_len,
            lowest_addr,
            ro_slices,
            expected,
        } in test_cases
        {
            let actual = build_owned_ro_section(buf_len, lowest_addr, ro_slices.iter().copied());
            assert_eq!(actual.as_slice(), expected, "{name}",);
        }
    }
}
