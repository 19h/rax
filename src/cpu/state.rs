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
    /// XMM registers (128-bit each, stored as [low, high])
    pub xmm: [[u64; 2]; 16],
    /// YMM upper 128-bits (bits 255:128 of YMM registers)
    pub ymm_high: [[u64; 2]; 16],
    /// MMX registers (64-bit each, aliased to low 64 bits of x87 FPU stack)
    pub mm: [u64; 8],
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

/// Complete x86_64 CPU state snapshot.
#[derive(Clone, Debug, Default)]
pub struct X86_64CpuState {
    pub regs: Registers,
    pub sregs: SystemRegisters,
}

/// Hexagon general and control registers.
#[derive(Clone, Debug)]
pub struct HexagonRegisters {
    pub r: [u32; 32],
    pub p: [bool; 4],
    pub c: [u32; 32],
}

impl Default for HexagonRegisters {
    fn default() -> Self {
        HexagonRegisters {
            r: [0u32; 32],
            p: [false; 4],
            c: [0u32; 32],
        }
    }
}

impl HexagonRegisters {
    pub fn pc(&self) -> u32 {
        self.c[9]
    }

    pub fn set_pc(&mut self, pc: u32) {
        self.c[9] = pc;
    }

    pub fn usr(&self) -> u32 {
        self.c[8]
    }

    pub fn set_usr(&mut self, value: u32) {
        self.c[8] = value;
    }

    pub fn predicate(&self, index: usize) -> bool {
        self.p[index]
    }

    pub fn set_predicate(&mut self, index: usize, value: bool) {
        self.p[index] = value;
        self.c[4] = self.pack_predicates();
    }

    pub fn control(&self, index: usize) -> u32 {
        if index == 4 {
            self.pack_predicates()
        } else {
            self.c[index]
        }
    }

    pub fn set_control(&mut self, index: usize, value: u32) {
        if index == 4 {
            self.unpack_predicates(value);
        } else {
            self.c[index] = value;
        }
    }

    fn pack_predicates(&self) -> u32 {
        let mut value = 0u32;
        for (idx, bit) in self.p.iter().enumerate() {
            if *bit {
                value |= 1u32 << idx;
            }
        }
        value
    }

    fn unpack_predicates(&mut self, value: u32) {
        for idx in 0..self.p.len() {
            self.p[idx] = (value >> idx) & 1 != 0;
        }
        self.c[4] = value & 0xF;
    }
}

/// Complete Hexagon CPU state snapshot.
#[derive(Clone, Debug, Default)]
pub struct HexagonCpuState {
    pub regs: HexagonRegisters,
}

/// Architecture-specific CPU state snapshot.
#[derive(Clone, Debug)]
pub enum CpuState {
    X86_64(X86_64CpuState),
    Hexagon(HexagonCpuState),
}

impl CpuState {
    pub fn x86_64(regs: Registers, sregs: SystemRegisters) -> Self {
        CpuState::X86_64(X86_64CpuState { regs, sregs })
    }

    pub fn hexagon(regs: HexagonRegisters) -> Self {
        CpuState::Hexagon(HexagonCpuState { regs })
    }
}
