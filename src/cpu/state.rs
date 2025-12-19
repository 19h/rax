//! Backend-agnostic CPU state types.

/// General-purpose registers.
#[derive(Clone, Debug, Default)]
pub struct Registers {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
}

/// Segment descriptor.
#[derive(Clone, Debug, Default)]
pub struct Segment {
    pub base: u64,
    pub limit: u32,
    pub selector: u16,
    pub type_: u8,
    pub present: bool,
    pub dpl: u8,
    pub db: bool,
    pub s: bool,
    pub l: bool,
    pub g: bool,
    pub avl: bool,
    pub unusable: bool,
}

/// Descriptor table register (GDTR/IDTR).
#[derive(Clone, Debug, Default)]
pub struct DescriptorTable {
    pub base: u64,
    pub limit: u16,
}

/// System registers (control registers, segment registers, etc.).
#[derive(Clone, Debug, Default)]
pub struct SystemRegisters {
    pub cs: Segment,
    pub ds: Segment,
    pub es: Segment,
    pub fs: Segment,
    pub gs: Segment,
    pub ss: Segment,
    pub tr: Segment,
    pub ldt: Segment,
    pub gdt: DescriptorTable,
    pub idt: DescriptorTable,
    pub cr0: u64,
    pub cr2: u64,
    pub cr3: u64,
    pub cr4: u64,
    pub cr8: u64,
    pub efer: u64,
}

/// Complete CPU state snapshot.
#[derive(Clone, Debug, Default)]
pub struct CpuState {
    pub regs: Registers,
    pub sregs: SystemRegisters,
}
