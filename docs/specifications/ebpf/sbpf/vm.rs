#![allow(clippy::arithmetic_side_effects)]
// Derived from uBPF <https://github.com/iovisor/ubpf>
// Copyright 2015 Big Switch Networks, Inc
//      (uBPF: VM architecture, parts of the interpreter, originally in C)
// Copyright 2016 6WIND S.A. <quentin.monnet@6wind.com>
//      (Translation to Rust, MetaBuff/multiple classes addition, hashmaps for syscalls)
// Copyright 2020 Solana Maintainers <maintainers@solana.com>
//
// Licensed under the Apache License, Version 2.0 <http://www.apache.org/licenses/LICENSE-2.0> or
// the MIT license <http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Virtual machine for eBPF programs.

use crate::{
    ebpf,
    elf::Executable,
    error::{EbpfError, ProgramResult},
    interpreter::Interpreter,
    memory_region::MemoryMapping,
    program::{BuiltinFunction, BuiltinProgram, FunctionRegistry, SBPFVersion},
    static_analysis::{Analysis, DummyContextObject, RegisterTraceEntry},
};
// Re-export defaults for direct access without the module path.
pub use defaults::get_stack_frame_size;
use std::{collections::BTreeMap, fmt::Debug, marker::PhantomData, mem::offset_of, ptr};

#[cfg(feature = "shuttle-test")]
use shuttle::sync::Arc;
#[cfg(not(feature = "shuttle-test"))]
use std::sync::Arc;

/// Returns (and if not done before generates) the encryption key for the VM pointer
pub fn get_runtime_environment_key() -> i32 {
    cfg_select! {
        feature = "jit" => {
            cfg_select! {
                feature = "shuttle-test" => { use shuttle::rand::{thread_rng, Rng}; }
                _ => { use rand::{thread_rng, Rng}; }
            }
            static RUNTIME_ENVIRONMENT_KEY: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
            *RUNTIME_ENVIRONMENT_KEY.get_or_init(|| thread_rng().gen::<i32>() >> 1)
        }
        _ => 0
    }
}

/// Default VM configuration settings.
pub(crate) mod defaults {
    const DEFAULT_STACK_FRAME_SIZE: usize = 4_096;

    /// Returns the stack frame size in bytes.
    ///
    /// With the `conf-stack-frame-size` feature enabled, the size can be overridden
    /// at runtime via the `VM_STACK_FRAME_SIZE` environment variable. The value is
    /// read once and cached. If not set, the default is always returned.
    ///
    /// Note: the `conf-stack-frame-size` variant can't be `const fn` (it uses
    /// `OnceLock`), while the production variant is `const fn`. Callers that need
    /// `const` evaluation (e.g. array sizes, const generics) should be aware that
    /// those uses will not compile when `conf-stack-frame-size` is enabled.
    #[cfg(feature = "conf-stack-frame-size")]
    #[inline(always)]
    pub fn get_stack_frame_size() -> usize {
        static STACK_FRAME_SIZE_CACHE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        *STACK_FRAME_SIZE_CACHE.get_or_init(|| {
            let size = std::env::var("VM_STACK_FRAME_SIZE")
                .ok()
                .and_then(|v| {
                    v.parse::<usize>().ok().filter(|sfz| *sfz > 0).or_else(|| {
                        log::warn!(
                            "Invalid VM_STACK_FRAME_SIZE={}, falling back to {}.",
                            v,
                            DEFAULT_STACK_FRAME_SIZE
                        );
                        None
                    })
                })
                .unwrap_or(DEFAULT_STACK_FRAME_SIZE);
            if size != DEFAULT_STACK_FRAME_SIZE {
                log::warn!(
                    "VM_STACK_FRAME_SIZE is set to {} (default: {}).",
                    size,
                    DEFAULT_STACK_FRAME_SIZE
                );
            }
            size
        })
    }

    /// Returns the stack frame size in bytes.
    #[cfg(not(feature = "conf-stack-frame-size"))]
    pub const fn get_stack_frame_size() -> usize {
        DEFAULT_STACK_FRAME_SIZE
    }
}

/// Specify the execution method.
pub enum ExecutionMode {
    /// Execute the program in an interpreted mode.
    Interpreted,
    /// Execute the program in JIT mode.
    ///
    /// The program must be JIT compiled.
    Jit,
    /// Allow JIT execution, if compiled. Otherwise fallback to interpreted.
    PreferJit,
}

/// VM configuration settings
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Maximum call depth
    pub max_call_depth: usize,
    /// Size of a stack frame in bytes, must match the size specified in the LLVM BPF backend
    pub stack_frame_size: usize,
    /// Enables the use of MemoryMapping and MemoryRegion for address translation
    pub enable_address_translation: bool,
    /// Enables gaps in VM address space between the stack frames
    pub enable_stack_frame_gaps: bool,
    /// Maximal pc distance after which a new instruction meter validation is emitted by the JIT
    pub instruction_meter_checkpoint_distance: usize,
    /// Enable instruction meter and limiting
    pub enable_instruction_meter: bool,
    /// Enable dynamic string allocation for labels
    pub enable_symbol_and_section_labels: bool,
    /// Reject ELF files containing issues that the verifier did not catch before (up to v0.2.21)
    pub reject_broken_elfs: bool,
    #[cfg(feature = "jit")]
    /// Ratio of native host instructions per random no-op in JIT (0 = OFF)
    pub noop_instruction_rate: u32,
    #[cfg(feature = "jit")]
    /// Enable disinfection of immediate values and offsets provided by the user in JIT
    pub sanitize_user_provided_values: bool,
    /// Avoid copying read only sections when possible
    pub optimize_rodata: bool,
    /// Use aligned memory mapping
    pub aligned_memory_mapping: bool,
    /// Allowed [SBPFVersion]s
    pub enabled_sbpf_versions: std::ops::RangeInclusive<SBPFVersion>,
}

impl Config {
    /// Returns the size of the stack memory region
    pub fn stack_size(&self) -> usize {
        self.stack_frame_size * self.max_call_depth
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_call_depth: 64,
            stack_frame_size: defaults::get_stack_frame_size(),
            enable_address_translation: true,
            enable_stack_frame_gaps: true,
            instruction_meter_checkpoint_distance: 10000,
            enable_instruction_meter: true,
            enable_symbol_and_section_labels: false,
            reject_broken_elfs: false,
            #[cfg(feature = "jit")]
            noop_instruction_rate: 256,
            #[cfg(feature = "jit")]
            sanitize_user_provided_values: true,
            optimize_rodata: true,
            aligned_memory_mapping: false,
            enabled_sbpf_versions: SBPFVersion::V0..=SBPFVersion::V4,
        }
    }
}

/// Static constructors for Executable
impl<C: ContextObject> Executable<C> {
    /// Creates an executable from an ELF file
    pub fn from_elf(elf_bytes: &[u8], loader: Arc<BuiltinProgram<C>>) -> Result<Self, EbpfError> {
        let executable = Executable::load(elf_bytes, loader)?;
        Ok(executable)
    }
    /// Creates an executable from machine code
    pub fn from_text_bytes(
        text_bytes: &[u8],
        loader: Arc<BuiltinProgram<C>>,
        sbpf_version: SBPFVersion,
        function_registry: FunctionRegistry<usize>,
    ) -> Result<Self, EbpfError> {
        Executable::new_from_text_bytes(text_bytes, loader, sbpf_version, function_registry)
            .map_err(EbpfError::ElfError)
    }
}

/// Runtime context
pub trait ContextObject {
    /// Consume instructions from meter
    fn consume(&mut self, amount: u64);
    /// Get the number of remaining instructions allowed
    fn get_remaining(&self) -> u64;
    /// Return a mutable pointer to the active MemoryMapping
    fn active_mapping_ptr(&mut self) -> ptr::NonNull<MemoryMapping>;
}

/// Statistic of taken branches (from a recorded trace)
pub struct DynamicAnalysis {
    /// Maximal edge counter value
    pub edge_counter_max: usize,
    /// src_node, dst_node, edge_counter
    pub edges: BTreeMap<usize, BTreeMap<usize, usize>>,
}

impl DynamicAnalysis {
    /// Accumulates a trace
    pub fn new(register_trace: &[[u64; 12]], analysis: &Analysis) -> Self {
        let mut result = Self {
            edge_counter_max: 0,
            edges: BTreeMap::new(),
        };
        let mut last_basic_block = usize::MAX;
        for traced_instruction in register_trace.iter() {
            let pc = traced_instruction[11] as usize;
            if analysis.cfg_nodes.contains_key(&pc) {
                let counter = result
                    .edges
                    .entry(last_basic_block)
                    .or_default()
                    .entry(pc)
                    .or_insert(0);
                *counter += 1;
                result.edge_counter_max = result.edge_counter_max.max(*counter);
                last_basic_block = pc;
            }
        }
        result
    }
}

/// A call frame used for function calls inside the Interpreter
#[derive(Clone, Default)]
pub struct CallFrame {
    /// The caller saved registers
    pub caller_saved_registers: [u64; ebpf::SCRATCH_REGS],
    /// The callers frame pointer
    pub frame_pointer: u64,
    /// The target_pc of the exit instruction which returns back to the caller
    pub target_pc: u64,
}

/// Indices of slots inside [EbpfVm]
pub enum RuntimeEnvironmentSlot {
    /// [EbpfVm::host_stack_pointer]
    HostStackPointer = offset_of!(EbpfVm<DummyContextObject>, host_stack_pointer) as isize,
    /// [EbpfVm::call_depth]
    CallDepth = offset_of!(EbpfVm<DummyContextObject>, call_depth) as isize,
    /// [EbpfVm::context_object_pointer]
    ContextObjectPointer = offset_of!(EbpfVm<DummyContextObject>, context_object_pointer) as isize,
    /// [EbpfVm::previous_instruction_meter]
    PreviousInstructionMeter =
        offset_of!(EbpfVm<DummyContextObject>, previous_instruction_meter) as isize,
    /// [EbpfVm::due_insn_count]
    DueInsnCount = offset_of!(EbpfVm<DummyContextObject>, due_insn_count) as isize,
    /// [EbpfVm::stopwatch_numerator]
    StopwatchNumerator = offset_of!(EbpfVm<DummyContextObject>, stopwatch_numerator) as isize,
    /// [EbpfVm::stopwatch_denominator]
    StopwatchDenominator = offset_of!(EbpfVm<DummyContextObject>, stopwatch_denominator) as isize,
    /// [EbpfVm::registers]
    Registers = offset_of!(EbpfVm<DummyContextObject>, registers) as isize,
    /// [EbpfVm::program_result]
    ProgramResult = offset_of!(EbpfVm<DummyContextObject>, program_result) as isize,
    /// [EbpfVm::memory_mapping]
    MemoryMapping = offset_of!(EbpfVm<DummyContextObject>, memory_mapping) as isize,
    /// [EbpfVm::register_trace]
    RegisterTrace = offset_of!(EbpfVm<DummyContextObject>, register_trace) as isize,
}

/// A virtual machine to run eBPF programs.
///
/// # Examples
///
/// ```
/// use solana_sbpf::{
///     aligned_memory::AlignedMemory,
///     ebpf,
///     elf::Executable,
///     memory_region::{MemoryMapping, MemoryRegion},
///     program::{BuiltinProgram, FunctionRegistry, SBPFVersion},
///     verifier::RequisiteVerifier,
///     vm::{CallFrame, Config, EbpfVm, ExecutionMode},
/// };
/// use test_utils::TestContextObject;
///
/// let prog = &[
///     0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // add64 r0, 0
///     0x95, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00  // exit
/// ];
/// let mut mem: [u8; _] = [0xaa, 0xbb, 0x11, 0x22, 0xcc, 0xdd];
///
/// let loader = std::sync::Arc::new(BuiltinProgram::new_mock());
/// let function_registry = FunctionRegistry::default();
/// let mut executable = Executable::<TestContextObject>::from_text_bytes(prog, loader.clone(), SBPFVersion::V4, function_registry).unwrap();
/// executable.verify::<RequisiteVerifier>().unwrap();
/// let mut context_object = TestContextObject::new(2);
/// let sbpf_version = executable.get_sbpf_version();
///
/// let mut stack = AlignedMemory::<{ebpf::HOST_ALIGN}>::zero_filled(executable.get_config().stack_size());
/// let stack_len = stack.len();
/// let mut heap = AlignedMemory::<{ebpf::HOST_ALIGN}>::with_capacity(0);
///
/// let regions: Vec<MemoryRegion> = vec![
///     executable.get_ro_region(),
///     MemoryRegion::new(&mut stack, ebpf::MM_STACK_START),
///     MemoryRegion::new(&mut heap, ebpf::MM_HEAP_START),
///     MemoryRegion::new(&raw mut mem, ebpf::MM_INPUT_START),
/// ];
///;
/// context_object.memory_mapping = unsafe {
///     MemoryMapping::new(regions, executable.get_config(), sbpf_version).unwrap()
/// };
///
/// let mut vm = EbpfVm::new(loader, sbpf_version, &mut context_object, stack_len);
///
/// let mut call_frames = vec![CallFrame::default(); executable.get_config().max_call_depth];
/// let (instruction_count, result) = vm.execute_program(
///     &executable,
///     &mut ExecutionMode::Interpreted,
///     &mut call_frames,
/// );
/// assert_eq!(instruction_count, 2);
/// assert_eq!(result.unwrap(), 0);
/// ```
#[repr(C)]
pub struct EbpfVm<'a, C: ContextObject> {
    /// Needed to exit from the guest back into the host
    pub host_stack_pointer: *mut u64,
    /// The current call depth.
    ///
    /// Incremented on calls and decremented on exits. It's used to enforce
    /// config.max_call_depth and to know when to terminate execution.
    pub call_depth: u64,
    /// Pointer to ContextObject
    pub(crate) context_object_pointer: ptr::NonNull<C>,
    /// The lifetime for the context object pointer
    context_object_lifetime: PhantomData<&'a mut C>,
    /// Last return value of instruction_meter.get_remaining()
    pub previous_instruction_meter: u64,
    /// Outstanding value to instruction_meter.consume()
    pub due_insn_count: u64,
    /// CPU cycles accumulated by the stop watch
    pub stopwatch_numerator: u64,
    /// Number of times the stop watch was used
    pub stopwatch_denominator: u64,
    /// Registers inlined
    pub registers: [u64; 12],
    /// ProgramResult inlined
    pub program_result: ProgramResult,
    /// MemoryMapping inlined
    pub(crate) memory_mapping: ptr::NonNull<MemoryMapping>,
    /// Loader built-in program
    pub loader: Arc<BuiltinProgram<C>>,
    /// Collector for the instruction trace
    pub register_trace: Vec<RegisterTraceEntry>,
    /// TCP port for the debugger interface
    #[cfg(feature = "debugger")]
    pub debug_port: Option<u16>,
    /// Debug metadata passed
    #[cfg(feature = "debugger")]
    pub debug_metadata: Option<String>,
}

impl<'a, C: ContextObject> EbpfVm<'a, C> {
    /// Creates a new virtual machine instance.
    pub fn new(
        loader: Arc<BuiltinProgram<C>>,
        sbpf_version: SBPFVersion,
        context_object: &'a mut C,
        stack_len: usize,
    ) -> Self {
        let config = loader.get_config();
        let mut registers = [0u64; 12];
        registers[ebpf::FRAME_PTR_REG] =
            ebpf::MM_STACK_START.saturating_add(if !sbpf_version.manual_stack_frame_bump() {
                config.stack_frame_size
            } else {
                stack_len
            } as u64);

        let memory_mapping = context_object.active_mapping_ptr();
        EbpfVm {
            host_stack_pointer: std::ptr::null_mut(),
            call_depth: 0,
            context_object_pointer: ptr::NonNull::from_mut(context_object),
            context_object_lifetime: PhantomData,
            previous_instruction_meter: 0,
            due_insn_count: 0,
            stopwatch_numerator: 0,
            stopwatch_denominator: 0,
            registers,
            program_result: ProgramResult::Ok(0),
            memory_mapping,
            loader,
            #[cfg(feature = "debugger")]
            debug_port: std::env::var("VM_DEBUG_PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok()),
            #[cfg(feature = "debugger")]
            debug_metadata: None,
            register_trace: Vec::default(),
        }
    }

    /// Execute the program
    ///
    /// Use `mode` parameter to request a specific execution type. This function will write back
    /// the execution mode used back to the reference passed in.
    ///
    /// It is required to provide `call_frames` when executing in interpreted mode.
    /// `call_frames` must be large enough to hold the executable config's `max_call_depth`
    /// frames.
    ///
    /// Returns the instruction meter count (CUs) and the execution result of the program.
    pub fn execute_program(
        &mut self,
        executable: &Executable<C>,
        mode: &mut ExecutionMode,
        call_frames: &mut [CallFrame],
    ) -> (u64, ProgramResult) {
        debug_assert!(Arc::ptr_eq(&self.loader, executable.get_loader()));
        self.registers[11] = executable.get_entrypoint_instruction_offset() as u64;
        let config = executable.get_config();
        let initial_insn_count = self.context().get_remaining();
        self.previous_instruction_meter = initial_insn_count;
        self.due_insn_count = 0;
        self.program_result = ProgramResult::Ok(0);

        'execute: {
            match *mode {
                ExecutionMode::Interpreted => {}

                #[cfg(all(feature = "jit", not(target_os = "windows"), target_arch = "x86_64"))]
                ExecutionMode::PreferJit => {
                    if let Some(compiled_program) = executable.get_compiled_program() {
                        *mode = ExecutionMode::Jit;
                        break 'execute compiled_program.invoke(config, self, self.registers);
                    }
                }
                #[cfg(not(all(
                    feature = "jit",
                    not(target_os = "windows"),
                    target_arch = "x86_64"
                )))]
                ExecutionMode::PreferJit => {}

                #[cfg(all(feature = "jit", not(target_os = "windows"), target_arch = "x86_64"))]
                ExecutionMode::Jit => {
                    let Some(compiled_program) = executable.get_compiled_program() else {
                        return (0, ProgramResult::Err(EbpfError::JitNotCompiled));
                    };
                    *mode = ExecutionMode::Jit;
                    break 'execute compiled_program.invoke(config, self, self.registers);
                }
                #[cfg(not(all(
                    feature = "jit",
                    not(target_os = "windows"),
                    target_arch = "x86_64"
                )))]
                ExecutionMode::Jit => return (0, ProgramResult::Err(EbpfError::JitNotCompiled)),
            }

            *mode = ExecutionMode::Interpreted;
            let interpreter = Interpreter::new(self, executable, self.registers, call_frames);
            break 'execute run_interpreter(interpreter);
        }

        let instruction_count = if config.enable_instruction_meter {
            let due_insn_count = self.due_insn_count;
            let context = self.context();
            context.consume(due_insn_count);
            initial_insn_count.saturating_sub(context.get_remaining())
        } else {
            0
        };
        let mut result = ProgramResult::Ok(0);
        std::mem::swap(&mut result, &mut self.program_result);
        (instruction_count, result)
    }

    /// Invokes a built-in function
    pub fn invoke_function(&mut self, function: BuiltinFunction<C>) {
        function(
            self.encrypted_host_address(),
            self.registers[1],
            self.registers[2],
            self.registers[3],
            self.registers[4],
            self.registers[5],
        );
    }

    /// Build a `VmAddress` containing a (potentially) encrypted host pointer to self.
    ///
    /// Note that this type is effectively a mutable pointer to `self` and although it valid to
    /// create multiple of these addresses, using them to violate the Rust mutable references'
    /// uniqueness rule is not sound.
    pub(crate) fn encrypted_host_address(&mut self) -> EncryptedHostAddressToEbpfVm<C> {
        let addr = (&raw mut *self).expose_provenance() as isize;
        EncryptedHostAddressToEbpfVm(
            addr.wrapping_add(get_runtime_environment_key() as isize) as usize as u64,
            PhantomData,
        )
    }

    /// Get a reference to the context object referenced by this EbpfVm.
    pub fn context(&mut self) -> &mut C {
        // SAFETY: we've the unique reference to self here, so there can't be other live references
        // to `C` either, whether via the memory_mapping or the context_object_pointer itself.
        //
        // The `context_object_pointer` is pointing at a valid-to-dereference `C` at all times
        // through the EbpfVm lifetime.
        //
        // Note: for that reason we are intentionally tying the lifetime of the returned `C` to the
        // lifetime of `&mut self`, rather than returning `&'a mut C`, which would allow aliasing
        // the returned reference.
        unsafe { self.context_object_pointer.as_mut() }
    }

    // Intentionally not public. Users are expected to store their memory mapping inside – and
    // access from – C.
    pub(crate) fn memory(&mut self) -> &mut MemoryMapping {
        // SAFETY: we've the unique reference to self here, so there can't be other live references
        // to `C` either, whether via the memory_mapping or the context_object_pointer itself.
        //
        // The `context_object_pointer` is pointing at a valid-to-dereference `C` at all times
        // through the EbpfVm lifetime.
        unsafe { self.memory_mapping.as_mut() }
    }
}

/// Encrypted address to the [`EbpfVm`] object.
#[repr(transparent)]
pub struct EncryptedHostAddressToEbpfVm<C>(
    // This ends up having to be public to the crate because inline assembly wants to deal with
    // integers, not `VmAddress` (even though VmAddress has the same layout.)
    pub(crate) u64,
    PhantomData<C>,
);

impl<C: ContextObject> EncryptedHostAddressToEbpfVm<C> {
    /// Work on [`EbpfVm`] pointed to by this address.
    ///
    /// ## Safety
    ///
    /// Multiple concurrently live addresses can reference the same [`EbpfVm`] but under no
    /// circumstances may they be used to create multiple concurrent mutable references to the
    /// `EbpfVm`.
    pub unsafe fn with_vm<R>(&mut self, cb: impl FnOnce(&mut EbpfVm<'_, C>) -> R) -> R {
        let addr = (self.0 as usize as isize)
            .wrapping_sub(crate::vm::get_runtime_environment_key() as isize);
        // SAFETY: we've recovered the same pointer address as that of the reference used to
        // produce this offset address in the first place.
        // SAFETY: The mutable reference is unique due to invariant being passed onto the caller.
        let vm = unsafe {
            std::ptr::with_exposed_provenance_mut::<crate::vm::EbpfVm<C>>(addr as usize)
                .as_mut()
                .unwrap()
        };
        cb(vm)
    }
}

#[cold]
#[inline(never)]
fn run_interpreter<C: ContextObject>(mut interpreter: Interpreter<C>) {
    #[cfg(feature = "debugger")]
    if let Some(debug_port) = interpreter.vm.debug_port {
        return crate::debugger::execute(&mut interpreter, debug_port);
    }

    while interpreter.step() {}
    interpreter.vm.registers[11] = interpreter.reg[11];
}

#[cfg(test)]
mod tests {
    use crate::{
        memory_region::MemoryMapping,
        program::{BuiltinProgram, SBPFVersion},
        vm::{Config, ContextObject, RuntimeEnvironmentSlot},
    };
    use std::{ptr::NonNull, sync::Arc};

    #[test]
    fn test_runtime_environment_slots() {
        struct DummyContextObject(MemoryMapping);
        impl ContextObject for DummyContextObject {
            fn consume(&mut self, _: u64) {
                todo!()
            }
            fn get_remaining(&self) -> u64 {
                todo!()
            }
            fn active_mapping_ptr(&mut self) -> NonNull<MemoryMapping> {
                NonNull::from_mut(&mut self.0)
            }
        }
        let version = SBPFVersion::V4;
        let config = Config::default();
        let mut context_object =
            unsafe { DummyContextObject(MemoryMapping::new(vec![], &config, version).unwrap()) };
        let env = super::EbpfVm::new(
            Arc::new(BuiltinProgram::new_mock()),
            version,
            &mut context_object,
            4096,
        );

        macro_rules! check_slot {
            ($env:expr, $entry:ident, $slot:ident) => {
                assert_eq!(
                    unsafe {
                        std::ptr::addr_of!($env.$entry)
                            .cast::<u8>()
                            .offset_from(std::ptr::addr_of!($env).cast::<u8>()) as usize
                    },
                    RuntimeEnvironmentSlot::$slot as usize,
                );
            };
        }

        check_slot!(env, host_stack_pointer, HostStackPointer);
        check_slot!(env, call_depth, CallDepth);
        check_slot!(env, context_object_pointer, ContextObjectPointer);
        check_slot!(env, previous_instruction_meter, PreviousInstructionMeter);
        check_slot!(env, due_insn_count, DueInsnCount);
        check_slot!(env, stopwatch_numerator, StopwatchNumerator);
        check_slot!(env, stopwatch_denominator, StopwatchDenominator);
        check_slot!(env, registers, Registers);
        check_slot!(env, program_result, ProgramResult);
        check_slot!(env, memory_mapping, MemoryMapping);
        check_slot!(env, register_trace, RegisterTrace);
    }
}
