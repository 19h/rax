//! Common interface for built-in and user supplied programs

#[cfg(feature = "shuttle-test")]
use shuttle::rand;

use {
    crate::{
        ebpf,
        elf::ElfError,
        error::EbpfError,
        memory_management::{
            allocate_pages_pooled, free_pages_pooled, get_system_page_size, protect_pages,
            round_to_page_size, PagePermissions,
        },
        vm::{Config, ContextObject, EncryptedHostAddressToEbpfVm},
    },
    rand::{thread_rng, Rng},
    std::collections::{hash_map::Entry, HashMap},
    std::hash::{BuildHasher, Hasher},
    std::ptr::NonNull,
};

/// Defines a set of sbpf_version of an executable
#[derive(Debug, PartialEq, PartialOrd, Eq, Clone, Copy)]
pub enum SBPFVersion {
    /// The legacy format
    V0,
    /// SIMD-0166
    V1,
    /// SIMD-0174, SIMD-0173
    V2,
    /// SIMD-0178, SIMD-0189, SIMD-0377
    V3,
    /// SIMD-0177
    V4,
    /// Used for future versions
    Reserved,
}

impl SBPFVersion {
    /// Enable SIMD-0166: SBPF dynamic stack frames
    ///
    /// Allows usage of `add64 r10, imm`.
    pub const fn manual_stack_frame_bump(self) -> bool {
        matches!(self, SBPFVersion::V1 | SBPFVersion::V2)
    }
    /// ... SIMD-0166
    pub const fn stack_frame_gaps(self) -> bool {
        matches!(self, SBPFVersion::V0)
    }

    /// Enable SIMD-0174: SBPF arithmetics improvements
    pub const fn enable_pqr(self) -> bool {
        matches!(self, SBPFVersion::V2)
    }
    /// ... SIMD-0174
    pub const fn explicit_sign_extension_of_results(self) -> bool {
        matches!(self, SBPFVersion::V2)
    }
    /// ... SIMD-0174
    pub const fn swap_sub_reg_imm_operands(self) -> bool {
        matches!(self, SBPFVersion::V2)
    }
    /// ... SIMD-0174
    pub const fn disable_neg(self) -> bool {
        matches!(self, SBPFVersion::V2)
    }

    /// Enable SIMD-0173: SBPF instruction encoding improvements
    pub const fn callx_uses_src_reg(self) -> bool {
        matches!(self, SBPFVersion::V2)
    }
    /// ... SIMD-0173
    pub const fn disable_lddw(self) -> bool {
        matches!(self, SBPFVersion::V2)
    }
    /// ... SIMD-0173
    pub const fn disable_le(self) -> bool {
        matches!(self, SBPFVersion::V2)
    }
    /// ... SIMD-0173
    pub const fn move_memory_instruction_classes(self) -> bool {
        matches!(self, SBPFVersion::V2)
    }

    /// Enable SIMD-0178: SBPF Static Syscalls
    pub const fn static_syscalls(self) -> bool {
        matches!(self, SBPFVersion::V3 | SBPFVersion::V4)
    }
    /// Enable SIMD-0189: SBPF stricter ELF headers
    pub const fn enable_stricter_elf_headers(self) -> bool {
        matches!(self, SBPFVersion::V3 | SBPFVersion::V4)
    }
    /// ... SIMD-0189
    pub const fn enable_lower_rodata_vaddr(self) -> bool {
        matches!(self, SBPFVersion::V3 | SBPFVersion::V4)
    }
    /// ... SIMD-0377
    pub const fn enable_jmp32(self) -> bool {
        matches!(self, SBPFVersion::V3 | SBPFVersion::V4)
    }
    /// ... SIMD-0377
    pub const fn callx_uses_dst_reg(self) -> bool {
        matches!(self, SBPFVersion::V3 | SBPFVersion::V4)
    }

    /// Calculate the target program counter for a CALL_IMM instruction depending on
    /// the SBPF version.
    pub const fn calculate_call_imm_target_pc(self, pc: usize, imm: i64) -> u32 {
        if self.static_syscalls() {
            (pc as i64).saturating_add(imm).saturating_add(1) as u32
        } else {
            imm as u32
        }
    }
}

/// Map suitable to hold user-generated sets of function symbols in an [`Executable`].
///
/// [`Executable`]: crate::elf::Executable
#[derive(Debug, PartialEq, Eq)]
pub struct FunctionRegistry<T> {
    /// This map does not further hash the keys (which already are hashes), and instead randomizes
    /// them somewhat to avoid malicious exploitation of big-O factors in hashmaps.
    map: HashMap<u32, (Vec<u8>, T), KeyHasherRandomState>,
}

impl<T> Default for FunctionRegistry<T> {
    fn default() -> Self {
        Self {
            map: HashMap::default(),
        }
    }
}

#[derive(Clone, Copy)]
struct KeyHasherRandomState {
    multiplier: u64,
}

impl Default for KeyHasherRandomState {
    fn default() -> Self {
        Self {
            multiplier: thread_rng().gen::<u64>() | 1,
        }
    }
}

impl BuildHasher for KeyHasherRandomState {
    type Hasher = KeyHasher;
    fn build_hasher(&self) -> KeyHasher {
        KeyHasher {
            multiplier: self.multiplier,
            hash: 0,
        }
    }
}

struct KeyHasher {
    multiplier: u64,
    hash: u64,
}

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        // Only `write_u32` is used for the keys.
        for &byte in bytes {
            self.write_u32(self.hash as u32 ^ u32::from(byte));
        }
    }

    fn write_u32(&mut self, key: u32) {
        let product = u64::from(key).wrapping_mul(self.multiplier);
        self.hash = product ^ product >> 32;
    }
}

impl<T: Copy + PartialEq> FunctionRegistry<T> {
    /// Register a symbol with an explicit key
    pub fn register_function(
        &mut self,
        key: u32,
        name: impl Into<Vec<u8>>,
        value: T,
    ) -> Result<(), ElfError> {
        match self.map.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert((name.into(), value));
            }
            Entry::Occupied(entry) => {
                if entry.get().1 != value {
                    return Err(ElfError::SymbolHashCollision(key));
                }
            }
        }
        Ok(())
    }

    /// Used for transitioning from SBPFv0 to SBPFv3
    pub(crate) fn register_function_hashed_legacy<C: ContextObject>(
        &mut self,
        loader: &BuiltinProgram<C>,
        hash_symbol_name: bool,
        name: impl Into<Vec<u8>>,
        value: T,
    ) -> Result<u32, ElfError>
    where
        usize: From<T>,
    {
        let name = name.into();
        let config = loader.get_config();
        let key = if hash_symbol_name {
            let hash = if name == b"entrypoint" {
                ebpf::hash_symbol_name(b"entrypoint")
            } else {
                ebpf::hash_symbol_name(&usize::from(value).to_le_bytes())
            };
            if loader.get_function_registry().lookup_by_key(hash).is_some() {
                return Err(ElfError::SymbolHashCollision(hash));
            }
            hash
        } else {
            usize::from(value) as u32
        };
        self.register_function(
            key,
            if config.enable_symbol_and_section_labels || name == b"entrypoint" {
                name
            } else {
                Vec::default()
            },
            value,
        )?;
        Ok(key)
    }

    /// Unregister a symbol again
    pub fn unregister_function(&mut self, key: u32) -> bool {
        self.map.remove(&key).is_some()
    }

    /// Iterate over all keys
    pub fn keys(&self) -> impl Iterator<Item = u32> + '_ {
        self.map.keys().copied()
    }

    /// Iterate over all entries
    pub fn iter(&self) -> impl Iterator<Item = (u32, (&[u8], T))> + '_ {
        self.map
            .iter()
            .map(|(key, (name, value))| (*key, (name.as_slice(), *value)))
    }

    /// Get a function by its key
    #[inline]
    pub fn lookup_by_key(&self, key: u32) -> Option<(&[u8], T)> {
        self.map
            .get(&key)
            .map(|(function_name, value)| (function_name.as_slice(), *value))
    }

    /// Get a function by its name
    pub fn lookup_by_name(&self, name: &[u8]) -> Option<(&[u8], T)> {
        self.map
            .values()
            .find(|(function_name, _value)| function_name == name)
            .map(|(function_name, value)| (function_name.as_slice(), *value))
    }

    /// Calculate memory size
    pub fn mem_size(&self) -> usize {
        std::mem::size_of::<Self>().saturating_add(self.map.values().fold(
            0,
            |state: usize, (name, value)| {
                state.saturating_add(
                    std::mem::size_of_val(value).saturating_add(
                        std::mem::size_of_val(name).saturating_add(name.capacity()),
                    ),
                )
            },
        ))
    }
}

/// The JIT output for a program, in a single pooled allocation.
pub struct JitProgram {
    /// Size of the pooled allocation.
    allocation_size: usize,
    /// Offset to each BPF instruction's machine code within `text_section`.
    pc_section: NonNull<[u32]>,
    /// The executable machine code.
    text_section: NonNull<[u8]>,
    /// Whether [`Self::seal`] has been invoked.
    sealed: bool,
}

unsafe impl Send for JitProgram {}
unsafe impl Sync for JitProgram {}

impl JitProgram {
    /// Allocate a program with `pc` zeroed entries in the pc section and room for `code_capacity`
    /// bytes of machine code.
    pub fn new(pc: usize, code_capacity: usize) -> Self {
        let page_size = get_system_page_size();
        let pc_size = round_to_page_size(pc.saturating_mul(std::mem::size_of::<u32>()), page_size);
        let text_capacity = round_to_page_size(code_capacity, page_size);
        let (raw, allocation_size) = allocate_pages_pooled(pc_size.saturating_add(text_capacity));
        let raw = NonNull::new(raw).expect("the pooled allocation is never null");
        let text = unsafe { raw.add(pc_size) };
        let pc_section = NonNull::slice_from_raw_parts(raw.cast::<u32>(), pc);
        // The pc section relies on zero-initialization to distinguish unfilled forward-jump
        // targets from filled backward-jump targets in the old JIT. The pool may hand back
        // recycled memory. Zero just the pc section here.
        unsafe { std::ptr::write_bytes(pc_section.cast::<u32>().as_ptr(), 0, pc) };
        Self {
            allocation_size,
            pc_section,
            text_section: NonNull::slice_from_raw_parts(text, text_capacity),
            sealed: false,
        }
    }

    /// Offset in `text_section` for each BPF instruction.
    pub fn pc_section(&self) -> &[u32] {
        unsafe { self.pc_section.as_ref() }
    }

    /// The machine code, which is executable.
    pub fn text_section(&self) -> &[u8] {
        unsafe { self.text_section.as_ref() }
    }

    /// The pc section to fill in, which is empty once the program is sealed.
    pub fn pc_section_mut(&mut self) -> &mut [u32] {
        if self.sealed {
            return &mut [];
        }
        unsafe { self.pc_section.as_mut() }
    }

    /// The machine code to fill in, which is empty once the program is sealed.
    pub fn text_section_mut(&mut self) -> &mut [u8] {
        if self.sealed {
            return &mut [];
        }
        unsafe { self.text_section.as_mut() }
    }

    /// Make the pages read-only and read-execute, with `text_section` shrunk to the `used` length.
    ///
    /// Does nothing if the program is already sealed.
    pub fn seal(&mut self, used: usize) -> Result<(), EbpfError> {
        if self.sealed {
            return Ok(());
        }
        if used > self.text_section.len() {
            return Err(EbpfError::ExhaustedTextSegment(used));
        }
        let page_size = get_system_page_size();
        let pc_size = round_to_page_size(std::mem::size_of_val(self.pc_section()), page_size);
        let code_size = round_to_page_size(used, page_size);
        let text = self.text_section.as_ptr().cast::<u8>();
        let unused = unsafe { text.add(used) };

        // x64 debugger traps in the unused tail of the last code page.
        // FIXME: this should be architecture-independent somehow.
        unsafe { std::ptr::write_bytes(unused, 0xcc, code_size.wrapping_sub(used)) };

        unsafe {
            self.sealed = true;
            protect_pages(
                self.pc_section.as_ptr().cast::<u8>(),
                pc_size,
                PagePermissions::Read,
            )?;
            protect_pages(text, code_size, PagePermissions::ReadExecute)?;
        };
        self.text_section = NonNull::slice_from_raw_parts(self.text_section.cast::<u8>(), used);
        Ok(())
    }

    /// The length of the host machinecode in bytes
    pub fn machine_code_length(&self) -> usize {
        self.text_section().len()
    }

    /// The total pooled allocation size retained by the compiled program.
    pub fn mem_size(&self) -> usize {
        self.allocation_size
    }
}

impl Drop for JitProgram {
    fn drop(&mut self) {
        unsafe {
            free_pages_pooled(self.pc_section.as_ptr().cast::<u8>(), self.allocation_size);
        }
    }
}

impl std::fmt::Debug for JitProgram {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.write_fmt(format_args!("JitProgram {:?}", self as *const _))
    }
}

impl PartialEq for JitProgram {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Syscall handler function (ContextObject is derived from VM)
pub type BuiltinFunction<C> = fn(EncryptedHostAddressToEbpfVm<C>, u64, u64, u64, u64, u64);
/// Re-export of the JIT compiler for the declare_builtin_function! macro
#[cfg(all(feature = "jit", not(target_os = "windows"), target_arch = "x86_64"))]
pub type JitCompiler<'a, C> = crate::jit::JitCompiler<'a, C>;
/// Re-export of the JIT compiler for the declare_builtin_function! macro
#[cfg(not(all(feature = "jit", not(target_os = "windows"), target_arch = "x86_64")))]
pub struct JitCompiler<'a, C> {
    _phantom: std::marker::PhantomData<&'a C>,
}
#[cfg(not(all(feature = "jit", not(target_os = "windows"), target_arch = "x86_64")))]
impl<'a, C: ContextObject> JitCompiler<'a, C> {
    /// Dummy for declare_builtin_function!()
    #[allow(dead_code)]
    pub fn emit_external_call(&mut self, _function: BuiltinFunction<C>) {}
}
/// Syscall codegen function for JIT compiler
pub type BuiltinCodegen<C> = fn(&mut JitCompiler<C>);

/// Represents the interface to a fixed functionality program
#[derive(Eq)]
pub struct BuiltinProgram<C: ContextObject> {
    /// Holds the Config if this is a loader program
    config: Option<Box<Config>>,
    /// Function pointers by symbol with sparse indexing
    sparse_registry: FunctionRegistry<(BuiltinFunction<C>, BuiltinCodegen<C>)>,
}

impl<C: ContextObject> PartialEq for BuiltinProgram<C> {
    fn eq(&self, other: &Self) -> bool {
        self.config.eq(&other.config) && self.sparse_registry.eq(&other.sparse_registry)
    }
}

impl<C: ContextObject> BuiltinProgram<C> {
    /// Constructs a loader built-in program
    pub fn new_loader(config: Config) -> Self {
        Self {
            config: Some(Box::new(config)),
            sparse_registry: FunctionRegistry::default(),
        }
    }

    /// Constructs a built-in program
    pub fn new_builtin() -> Self {
        Self {
            config: None,
            sparse_registry: FunctionRegistry::default(),
        }
    }

    /// Constructs a mock loader built-in program
    pub fn new_mock() -> Self {
        Self {
            config: Some(Box::default()),
            sparse_registry: FunctionRegistry::default(),
        }
    }

    /// Get the configuration settings assuming this is a loader program
    pub fn get_config(&self) -> &Config {
        self.config.as_ref().unwrap()
    }

    /// Get the function registry depending on the SBPF version
    pub fn get_function_registry(
        &self,
    ) -> &FunctionRegistry<(BuiltinFunction<C>, BuiltinCodegen<C>)> {
        &self.sparse_registry
    }

    /// Calculate memory size
    pub fn mem_size(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(if self.config.is_some() {
                std::mem::size_of::<Config>()
            } else {
                0
            })
            .saturating_add(self.sparse_registry.mem_size())
    }

    /// Register a function both in the sparse and dense registries
    ///
    /// This is a low-level function. Prefer using [`Self::register_definition`].
    pub fn register_function(
        &mut self,
        name: &str,
        entry: (BuiltinFunction<C>, BuiltinCodegen<C>),
    ) -> Result<(), ElfError> {
        let key = ebpf::hash_symbol_name(name.as_bytes());
        self.sparse_registry
            .register_function(key, name, entry)
            .map(|_| ())
    }

    /// Register a function both in the sparse and dense registries
    pub fn register_definition<BFD: BuiltinFunctionDefinition<C>>(
        &mut self,
        name: &str,
    ) -> Result<(), ElfError> {
        self.register_function(name, (BFD::vm, BFD::codegen))
    }

    /// Remove a function by name (if it exists)
    pub fn unregister_function(&mut self, name: &str) -> bool {
        let key = ebpf::hash_symbol_name(name.as_bytes());
        self.sparse_registry.unregister_function(key)
    }
}

/// Native built-in functions that can be made available to programs to call.
pub trait BuiltinFunctionDefinition<C>
where
    C: crate::vm::ContextObject,
{
    /// Error type returned by this built-in function.
    type Error: Into<Box<dyn core::error::Error>>;

    /// The Rust side of the function logic.
    ///
    /// This is the only method you are required to override.
    fn rust(
        vm: &mut C,
        arg_a: u64,
        arg_b: u64,
        arg_c: u64,
        arg_d: u64,
        arg_e: u64,
    ) -> Result<u64, Self::Error>;

    /// The VM wrapper.
    #[expect(clippy::arithmetic_side_effects)]
    fn vm(mut vm: EncryptedHostAddressToEbpfVm<C>, a: u64, b: u64, c: u64, d: u64, e: u64) {
        unsafe {
            // SAFETY: Sound under the stacked lifetimes model – we've only one `VmAddress`.
            vm.with_vm(|vm| {
                let enable_insn_meter = vm.loader.get_config().enable_instruction_meter;
                if enable_insn_meter {
                    let used_cus = vm.previous_instruction_meter - vm.due_insn_count;
                    vm.context().consume(used_cus);
                }
                let converted_result: crate::error::ProgramResult =
                    Self::rust(vm.context(), a, b, c, d, e)
                        .map_err(|err| crate::error::EbpfError::SyscallError(err.into()))
                        .into();
                vm.program_result = converted_result;
                if enable_insn_meter {
                    vm.previous_instruction_meter = vm.context().get_remaining();
                }
            })
        }
    }

    /// Hook for the JIT compiler on how to codegen this built-in function.
    ///
    /// You could opt to codegen it in-line, but do note that defining the other methods is still
    /// required for non-JIT execution modes.
    fn codegen(jit: &mut JitCompiler<C>) {
        jit.emit_external_call(Self::vm);
    }

    /// Register this syscall to the provided program.
    fn register(program: &mut BuiltinProgram<C>, name: &str) -> Result<(), ElfError>
    where
        Self: Sized,
    {
        program.register_definition::<Self>(name)
    }
}

impl<C: ContextObject> std::fmt::Debug for BuiltinProgram<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        f.debug_struct("BuiltinProgram")
            .field("registry", unsafe {
                std::mem::transmute::<
                    &FunctionRegistry<(BuiltinFunction<C>, BuiltinCodegen<C>)>,
                    &FunctionRegistry<(usize, usize)>,
                >(&self.sparse_registry)
            })
            .finish()
    }
}

/// Generates an adapter for a BuiltinFunction between the Rust and the VM interface
#[macro_export]
macro_rules! declare_builtin_function {
    (
        $(#[$attr:meta])*
        $name:ident $(<$($generic_ident:tt : $generic_type:tt),+>)?,
        fn rust(
            $vm:ident : &mut $ContextObject:ty,
            $arg_a:ident : u64,
            $arg_b:ident : u64,
            $arg_c:ident : u64,
            $arg_d:ident : u64,
            $arg_e:ident : u64,
        ) -> Result<$Ok:ty, $Err:ty> {
            $($rust:tt)*
        }
        $(fn codegen($jit:ident : &mut JitCompiler<$ContextObject2:ty>) {
            $($codegen:tt)*
        })?
    ) => {
        $(#[$attr])*
        pub struct $name $(<$($generic_ident),+>)? (
            $(std::marker::PhantomData<($($generic_ident,)+)>)?
        );
        impl $(<$($generic_ident : $generic_type),+>)?
            $crate::program::BuiltinFunctionDefinition<$ContextObject> for
            $name $(<$($generic_ident),+>)?
        {
            type Error = $Err;
            fn rust(
                $vm: &mut $ContextObject,
                $arg_a: u64,
                $arg_b: u64,
                $arg_c: u64,
                $arg_d: u64,
                $arg_e: u64,
            ) -> core::result::Result<$Ok, $Err> {
                $($rust)*
            }
            $(fn codegen(
                $jit: &mut $crate::program::JitCompiler<$ContextObject2>,
            ) {
                $($codegen)*
            })?
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! check_lookups {
        ($registry:ident, $lens:expr) => {
            let key = |i: u32| ebpf::hash_symbol_name(&i.to_le_bytes());
            for len in $lens {
                let mut registry = $registry::default();
                for i in 0..len {
                    registry
                        .register_function(key(i), format!("f{i}"), i)
                        .unwrap();
                }
                let found = |registry: &$registry<u32>, i: u32| {
                    registry
                        .lookup_by_key(key(i))
                        .map(|(name, value)| (String::from_utf8(name.to_vec()).unwrap(), value))
                };
                for i in 0..len {
                    assert_eq!(found(&registry, i), Some((format!("f{i}"), i)));
                }
                for i in len..len.saturating_add(1000) {
                    assert_eq!(found(&registry, i), None);
                }
                let mut keys: Vec<_> = (0..len).map(key).collect();
                keys.sort();
                let mut registered: Vec<_> = registry.keys().collect();
                registered.sort();
                assert_eq!(registered, keys);
                let mut iterated: Vec<_> = registry.iter().map(|(key, _)| key).collect();
                iterated.sort();
                assert_eq!(iterated, keys);
                if len > 0 {
                    assert!(registry.register_function(key(0), "f0", 0).is_ok());
                    assert!(matches!(
                        registry.register_function(key(0), "f0", 1),
                        Err(ElfError::SymbolHashCollision(_))
                    ));
                    assert!(registry.unregister_function(key(0)));
                    assert!(!registry.unregister_function(key(0)));
                    assert_eq!(found(&registry, 0), None);
                    for i in 1..len {
                        assert_eq!(found(&registry, i), Some((format!("f{i}"), i)));
                    }
                    assert_eq!(registry.keys().count(), (len as usize).saturating_sub(1));
                    registry.register_function(key(0), "f0", 0).unwrap();
                    assert_eq!(found(&registry, 0), Some(("f0".to_string(), 0)));
                    for i in 0..len {
                        assert!(registry.unregister_function(key(i)));
                    }
                    assert_eq!(registry.keys().count(), 0);
                    for i in 0..len.saturating_add(1000) {
                        assert_eq!(found(&registry, i), None);
                    }
                }
            }
        };
    }

    #[test]
    fn function_registry_lookups() {
        check_lookups!(FunctionRegistry, [0u32, 1, 3, 64, 1000]);
    }
}
