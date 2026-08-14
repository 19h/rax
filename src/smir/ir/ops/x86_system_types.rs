//! Structured x86 system-operation payloads.

use crate::smir::ir::types::{Address, MemWidth, OpWidth, VReg};

/// `ENTER imm16, imm8` owns the complete implicit stack transaction. Keeping
/// the instruction as one operation is required for precise RSP/RBP commit and
/// for Intel's final-stack write-permission probe, which performs no data
/// write. `nesting_level` is the architecturally masked value in 0..=31.
#[derive(Clone, Debug)]
pub struct X86EnterOp {
    pub allocation_size: u16,
    pub nesting_level: u8,
    pub width: OpWidth,
    pub requires_apx: bool,
    /// Exact end of the source instruction, retained for native shape checks.
    pub next_pc: u64,
}

/// Operand width of x86 `LEAVE` in 64-bit mode. The architecture permits only
/// the default 64-bit form and the `66H`-selected 16-bit form; a 32-bit form is
/// not encodable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86LeaveWidth {
    W16,
    W64,
}

impl X86LeaveWidth {
    pub const fn bytes(self) -> u8 {
        match self {
            Self::W16 => 2,
            Self::W64 => 8,
        }
    }

    pub const fn mem_width(self) -> MemWidth {
        match self {
            Self::W16 => MemWidth::B2,
            Self::W64 => MemWidth::B8,
        }
    }
}

/// Complete long-mode x86 `LEAVE` transaction. The frame-pointer load and both
/// architectural register writes form one faulting instruction: #SS, #PF, and
/// #AC leave RSP and RBP unmodified. `next_pc` retains the exact source end for
/// native admission; REX2 forms additionally require dynamic APX support.
#[derive(Clone, Debug)]
pub struct X86LeaveOp {
    pub width: X86LeaveWidth,
    pub requires_apx: bool,
    pub next_pc: u64,
}

/// Direction of one implicit x86 FLAGS stack transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86StackFlagsKind {
    Push,
    Pop,
}

/// PUSHF/PUSHFQ and POPF/POPFQ own their complete implicit stack transaction.
/// A single operation is required because POPF privilege filtering can fault
/// after the memory read while leaving both RSP and RFLAGS uncommitted.
#[derive(Clone, Debug)]
pub struct X86StackFlagsOp {
    pub kind: X86StackFlagsKind,
    pub width: OpWidth,
    pub requires_apx: bool,
    /// Exact end of the source instruction, retained for native shape checks.
    pub next_pc: u64,
}

/// Architecturally readable x86 control registers accepted by `MOV r64, CRn`
/// in 64-bit mode. Reserved control-register numbers are represented as an
/// explicit invalid-opcode trap by the lifter and never reach this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86ControlReg {
    Cr0,
    Cr2,
    Cr3,
    Cr4,
    Cr8,
}

/// Encoded debug-register selector accepted by `MOV r64, DRn` and
/// `MOV DRn, r64`. DR4 and DR5 remain explicit because their CR4.DE-dependent
/// invalidity and DR6/DR7 alias behavior are architectural runtime state, not
/// static decode properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86DebugReg {
    Dr0,
    Dr1,
    Dr2,
    Dr3,
    Dr4,
    Dr5,
    Dr6,
    Dr7,
}

/// x86 RDTSC/RDTSCP timestamp read. Both forms write EDX:EAX with 32-bit
/// zero-extending writes. `dst_aux == Some(ECX)` selects RDTSCP: it additionally
/// reads guest IA32_TSC_AUX and has the architectural prior-load ordering
/// guarantee. `None` selects the unordered RDTSC form.
#[derive(Clone, Debug)]
pub struct X86ReadTscOp {
    pub dst_lo: VReg,
    pub dst_hi: VReg,
    pub dst_aux: Option<VReg>,
}

/// x86 RDPMC read under the deterministic legacy-PMU profile. `selector` is
/// ECX; the operation validates privilege and selector state before committing
/// zero-extended EDX:EAX destinations.
#[derive(Clone, Debug)]
pub struct X86ReadPmcOp {
    pub dst_lo: VReg,
    pub dst_hi: VReg,
    pub selector: VReg,
}

/// RDMSR/WRMSR implicit-register operation. `write == false` reads the MSR
/// selected by ECX into zero-extended EDX:EAX. `write == true` writes the low
/// 32-bit EDX:EAX pair, preserves all three GPRs, and terminates native
/// execution at the exact `next_pc` after a successful state transition.
#[derive(Clone, Debug)]
pub struct X86MsrOp {
    pub eax: VReg,
    pub ecx: VReg,
    pub edx: VReg,
    pub write: bool,
    pub next_pc: u64,
}

/// Architecturally distinct SMSW destinations. Register forms write the
/// selected 16-, 32-, or 64-bit GPR width; memory forms always store exactly
/// CR0[15:0] as a 2-byte quantity independently of the encoded operand size.
#[derive(Clone, Debug)]
pub enum X86SmswTarget {
    Register { dst: VReg, width: OpWidth },
    Memory { addr: Address },
}

/// SMSW reads implicit CR0 state after dynamic APX and UMIP checks. A REX2
/// encoding sets `requires_apx` even when it addresses a legacy GPR, because
/// the prefix itself is unavailable when the guest APX profile is disabled.
#[derive(Clone, Debug)]
pub struct X86SmswOp {
    pub target: X86SmswTarget,
    pub requires_apx: bool,
}

/// Architecturally readable x86 selector register. `Ldtr` and `Tr` are exposed
/// by SLDT/STR; the remaining variants are the visible segment selectors read
/// by `MOV r/m16/32/64, Sreg` (`8C /r`). The system variants remain first so
/// their established native-helper encodings stay append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86SystemSelector {
    Ldtr,
    Tr,
    Es,
    Cs,
    Ss,
    Ds,
    Fs,
    Gs,
}

/// Architecturally distinct selector-store destinations. Register forms write
/// the selected 16-, 32-, or 64-bit GPR width; ordinary memory forms always
/// store exactly the 16-bit selector independently of the encoded operand
/// size. Stack forms model long-mode `PUSH FS/GS` atomically: the selected
/// width is written at the decremented stack pointer, and the architectural
/// stack pointer is committed only after the write succeeds.
#[derive(Clone, Debug)]
pub enum X86SystemSelectorTarget {
    Register {
        dst: VReg,
        width: OpWidth,
    },
    Memory {
        addr: Address,
    },
    Stack {
        stack_pointer: VReg,
        width: MemWidth,
    },
}

/// Read one visible selector. SLDT/STR require protected-mode and UMIP
/// validation; `MOV r/m, Sreg` and `PUSH FS/GS` do not. Every REX2 encoding
/// requires the dynamic APX profile even when it addresses only legacy state.
#[derive(Clone, Debug)]
pub struct X86SystemSelectorStoreOp {
    pub selector: X86SystemSelector,
    pub target: X86SystemSelectorTarget,
    pub requires_apx: bool,
}

/// Selector-load source. Register forms always consume the low 16 bits. LLDT
/// and LTR memory forms are fixed at 2 bytes; `MOV Sreg,r/m` uses 2 bytes
/// unless REX.W/REX2.W selects an 8-byte memory read whose low 16 bits are
/// loaded. `stack_segment` preserves the architecturally distinct #SS(0)
/// classification for a noncanonical SS-based memory range. Stack forms model
/// long-mode `POP FS/GS` atomically: the width-selected read supplies the low
/// 16-bit selector, and the stack pointer commits only after the complete
/// segment load succeeds. Far-pointer forms read an offset followed by a
/// 16-bit selector for `LSS/LFS/LGS`; the width-tagged GPR destination commits
/// only after descriptor validation and the hidden-cache transition succeed.
#[derive(Clone, Debug)]
pub enum X86SystemSelectorSource {
    Register {
        src: VReg,
    },
    Memory {
        addr: Address,
        width: MemWidth,
        stack_segment: bool,
    },
    Stack {
        stack_pointer: VReg,
        width: MemWidth,
    },
    FarPointer {
        addr: Address,
        dst: VReg,
        offset_width: OpWidth,
        stack_segment: bool,
    },
}

/// Load one visible selector and its hidden descriptor cache. LLDT/LTR perform
/// their system-descriptor checks; `MOV Sreg,r/m` admits ES/SS/DS/FS/GS and
/// performs data/stack descriptor validation plus the implicit accessed-bit
/// transition; `POP FS/GS` additionally commits its stack-pointer increment
/// only after that transition; and `LSS/LFS/LGS` commit their paired GPR only
/// after that transition. LTR performs the available-to-busy GDT transition.
/// LLDT/LTR serialize; every variant terminates native execution and hands off
/// at `next_pc` so the updated state is visible before later guest work.
#[derive(Clone, Debug)]
pub struct X86SystemSelectorLoadOp {
    pub selector: X86SystemSelector,
    pub source: X86SystemSelectorSource,
    pub requires_apx: bool,
    pub next_pc: u64,
}

/// Access predicate selected by Group-6 VERR/VERW. Descriptor presence is not
/// part of either predicate; only descriptor type, access bits, and privilege
/// determine the ZF result after a selector names an in-bounds descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86SelectorVerifyKind {
    Read,
    Write,
}

/// Fixed-width VERR/VERW selector source. Register forms consume the low 16
/// bits. Memory forms read exactly 2 bytes, with `stack_segment` retaining the
/// #SS(0) versus #GP(0) distinction for a noncanonical long-mode range.
#[derive(Clone, Debug)]
pub enum X86SelectorVerifySource {
    Register { src: VReg },
    Memory { addr: Address, stack_segment: bool },
}

/// VERR/VERW verify a code/data selector without loading it. Invalid selectors
/// and descriptors commit ZF=0 rather than a selector-derived exception; source
/// and descriptor-table memory accesses remain faulting. Every REX2 encoding
/// requires the dynamic APX profile, and `next_pc` records the exact strict-lift
/// instruction boundary for native shape validation.
#[derive(Clone, Debug)]
pub struct X86SelectorVerifyOp {
    pub kind: X86SelectorVerifyKind,
    pub source: X86SelectorVerifySource,
    pub requires_apx: bool,
    pub next_pc: u64,
}

/// Descriptor value selected by LAR/LSL after their non-faulting selector
/// checks. AccessRights retains Intel's architecturally undefined result bits
/// 19:16 as the deterministic descriptor image used by the direct engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86SelectorQueryKind {
    AccessRights,
    Limit,
}

/// Fixed-width LAR/LSL selector source. The source is always truncated to 16
/// bits independently of the destination operand size.
#[derive(Clone, Debug)]
pub enum X86SelectorQuerySource {
    Register { src: VReg },
    Memory { addr: Address, stack_segment: bool },
}

/// LAR/LSL atomically combine a faulting source read, one implicit 8- or
/// 16-byte descriptor read, a conditional width-specific GPR write, and a ZF
/// update. Invalid selectors leave `dst` unchanged and commit ZF=0.
#[derive(Clone, Debug)]
pub struct X86SelectorQueryOp {
    pub kind: X86SelectorQueryKind,
    pub dst: VReg,
    pub source: X86SelectorQuerySource,
    pub width: OpWidth,
    pub requires_apx: bool,
    pub next_pc: u64,
}

/// Indirect far JMP (`FF /5`) through a memory far pointer. The strict x86-64
/// lifter records the encoded 16-, 32-, or 64-bit offset width and produces the
/// target in architectural RIP. Descriptor-table reads, optional call-gate
/// indirection, the implicit code-descriptor accessed-bit write, and CS:RIP
/// commit are one fault-precise operation.
#[derive(Clone, Debug)]
pub struct X86FarJumpOp {
    pub addr: Address,
    pub target: VReg,
    pub offset_width: OpWidth,
    pub requires_apx: bool,
    /// Select #SS(0), rather than #GP(0), when the far-pointer linear address
    /// is noncanonical because the effective address uses SS.
    pub stack_segment: bool,
    /// Exact end of the source instruction, retained for native shape checks.
    pub next_pc: u64,
}

/// Indirect far CALL (`FF /3`) through a memory far pointer. Direct code
/// targets push a width-selected CS:return-IP frame; IA-32e call gates use
/// fixed 64-bit entries and may select a more-privileged TSS stack. Pointer,
/// descriptor, TSS, stack, accessed-bit, and CS:RIP:RSP[:SS] effects form one
/// fault-precise operation.
#[derive(Clone, Debug)]
pub struct X86FarCallOp {
    pub addr: Address,
    pub target: VReg,
    pub offset_width: OpWidth,
    pub requires_apx: bool,
    /// Select #SS(0), rather than #GP(0), for a noncanonical far-pointer range
    /// whose default segment is SS.
    pub stack_segment: bool,
    /// Architectural return address and exact source-instruction end.
    pub next_pc: u64,
}

/// Far RET (`CA`/`CB`). The operation owns all width-selected stack reads,
/// code/stack descriptor validation, accessed-bit transitions, optional outer
/// privilege stack restoration, data-segment invalidation, and the terminal
/// CS:RIP:RSP[:SS] commit.
#[derive(Clone, Debug)]
pub struct X86FarReturnOp {
    pub target: VReg,
    pub offset_width: OpWidth,
    /// Immediate parameter-release count. `CB` and `CA 00 00` are
    /// architecturally equivalent apart from source length.
    pub pop_bytes: u16,
    pub requires_apx: bool,
    /// Exact end of the source instruction, retained for native shape checks.
    pub next_pc: u64,
}

/// Intel fast system-transfer instruction selected by map-1 opcodes `0F 34`
/// and `0F 35` under RAX's fixed `GenuineIntel` guest profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86FastSystemTransferKind {
    Sysenter,
    Sysexit,
}

/// Fault-precise SYSENTER/SYSEXIT state transition. Both forms write
/// architectural RIP and RSP and install fixed CS/SS descriptor caches.
/// SYSEXIT additionally consumes RCX/RDX as its return RSP/RIP sources;
/// SYSENTER instead consumes the implicit IA32_SYSENTER_* state. `operand64`
/// is meaningful only for SYSEXIT and selects its REX.W return-to-64-bit form.
#[derive(Clone, Debug)]
pub struct X86FastSystemTransferOp {
    pub kind: X86FastSystemTransferKind,
    pub target: VReg,
    pub stack_pointer: VReg,
    pub return_target: VReg,
    pub return_stack_pointer: VReg,
    pub operand64: bool,
    /// Exact end of the source instruction, retained for native shape checks.
    pub next_pc: u64,
}

/// Architecturally distinct LMSW sources. Both forms read exactly 16 bits;
/// operand-size prefixes never change the source width.
#[derive(Clone, Debug)]
pub enum X86LmswSource {
    Register { src: VReg },
    Memory { addr: Address },
}

/// LMSW reads its source only after dynamic APX and CPL validation, updates
/// CR0[3:0] without clearing an already-set CR0.PE, serializes execution, and
/// hands native execution off at the exact next instruction.
#[derive(Clone, Debug)]
pub struct X86LmswOp {
    pub source: X86LmswSource,
    pub requires_apx: bool,
    pub next_pc: u64,
}

/// Descriptor-table register selected by SGDT/SIDT/LGDT/LIDT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86DescriptorTable {
    Gdt,
    Idt,
}

/// SGDT/SIDT store the selected implicit descriptor-table register through a
/// memory-only operand. `SourceArch::X86_64` fixes the payload at 10 bytes:
/// the 16-bit limit followed by the 64-bit base. A REX2 encoding requires the
/// dynamic APX profile even when every address component is a legacy GPR.
#[derive(Clone, Debug)]
pub struct X86DescriptorTableStoreOp {
    pub addr: Address,
    pub table: X86DescriptorTable,
    pub requires_apx: bool,
}

/// LGDT/LIDT load the selected implicit descriptor-table register from one
/// memory-only operand. `SourceArch::X86_64` fixes the payload at 10 bytes:
/// the 16-bit limit followed by the complete 64-bit base. A successful native
/// execution serializes and hands off at `next_pc`; every fault restarts at the
/// original instruction without committing either field.
#[derive(Clone, Debug)]
pub struct X86DescriptorTableLoadOp {
    pub addr: Address,
    pub table: X86DescriptorTable,
    pub requires_apx: bool,
    pub next_pc: u64,
}

/// INVLPG names a linear page without reading or writing the addressed byte.
/// Dynamic APX and CPL validation precede the invalidation. In 64-bit mode a
/// non-canonical effective address is architecturally a successful no-op. A
/// successful native execution terminates at `next_pc` so subsequent fetches
/// and data accesses observe the updated translation-cache state.
#[derive(Clone, Debug)]
pub struct X86InvlpgOp {
    pub addr: Address,
    pub requires_apx: bool,
    pub next_pc: u64,
}

/// INVPCID reads one 128-bit descriptor after APX and CPL validation, validates
/// the register-selected invalidation type and descriptor fields, then
/// synchronizes translation-dependent state. `stack_segment` retains the
/// #SS(0)/#GP(0) distinction for a noncanonical 16-byte source range. Native
/// success terminates at `next_pc`; every fault replays at the original PC.
#[derive(Clone, Debug)]
pub struct X86InvpcidOp {
    pub invpcid_type: VReg,
    pub addr: Address,
    pub requires_apx: bool,
    pub stack_segment: bool,
    pub next_pc: u64,
}

/// MONITOR/MWAIT under the deterministic guest profile. `Some(addr)` is
/// MONITOR: `hint` is EDX, validate CPL/RCX, then perform an ordered faulting
/// byte read from the monitored linear address. `None` is MWAIT: `hint` is
/// EAX, validate CPL/RCX, and return immediately because the emulator does not
/// retain monitor hardware state. Hint values are implementation-dependent
/// and ignored by this profile. CPUID.05H advertises no MWAIT extensions, so
/// RCX must be zero for both forms in 64-bit mode.
#[derive(Clone, Debug)]
pub struct X86MonitorMwaitOp {
    pub rcx: VReg,
    pub hint: VReg,
    pub addr: Option<Address>,
    /// MONITOR used an SS override, selecting #SS(0) rather than #GP(0) for
    /// a noncanonical 64-bit linear address. Always false for MWAIT.
    pub stack_segment: bool,
}

/// WAITPKG under the deterministic guest profile. UMONITOR performs the
/// architecturally ordered, faulting byte probe but does not retain monitor
/// hardware state. UMWAIT and TPAUSE validate their explicit 32-bit control
/// source and CR4.TSD privilege state, read the implicit EDX:EAX deadline, and
/// return immediately with CF/PF/AF/ZF/SF/OF cleared. An implementation-
/// dependent wake event is permitted to end either wait before its deadline.
#[derive(Clone, Debug)]
pub enum X86WaitPkgOp {
    Umonitor {
        addr: Address,
        /// An SS override selects #SS(0), rather than #GP(0), for a
        /// noncanonical 64-bit linear address.
        stack_segment: bool,
    },
    Umwait {
        control: VReg,
        deadline_low: VReg,
        deadline_high: VReg,
    },
    Tpause {
        control: VReg,
        deadline_low: VReg,
        deadline_high: VReg,
    },
}
