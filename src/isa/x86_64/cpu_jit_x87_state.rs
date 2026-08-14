//! Exact x87 environment marshalling for native x86-64 regions.

use super::{FpuState, X86_64Vcpu};
use crate::smir::lower::runtime::GuestRegs;

impl X86_64Vcpu {
    pub(super) fn marshal_x87_environment_to_guest_regs(&self, gr: &mut GuestRegs) {
        gr.x87_control_word = u64::from(self.fpu.control_word);
        gr.x87_status_word = u64::from(self.fpu.status_word);
        gr.x87_tag_word = u64::from(self.fpu.tag_word);
        gr.x87_data_ptr = self.fpu.data_ptr;
        gr.x87_instr_ptr = self.fpu.instr_ptr;
        gr.x87_last_opcode = u64::from(self.fpu.last_opcode);
    }

    pub(super) fn marshal_x87_environment_from_guest_regs(&mut self, gr: &GuestRegs) {
        self.fpu.control_word = gr.x87_control_word as u16;
        self.fpu.status_word = gr.x87_status_word as u16;
        self.fpu.tag_word = gr.x87_tag_word as u16;
        self.fpu.data_ptr = gr.x87_data_ptr;
        self.fpu.instr_ptr = gr.x87_instr_ptr;
        self.fpu.last_opcode = gr.x87_last_opcode as u16;
        self.fpu.top = ((self.fpu.status_word >> 11) & 7) as u8;
    }
}

impl FpuState {
    pub(super) fn append_jit_verify_diffs(&self, native: &Self, diffs: &mut Vec<String>) {
        for (name, interp, jit) in [
            (
                "x87_control_word",
                u64::from(self.control_word),
                u64::from(native.control_word),
            ),
            (
                "x87_status_word",
                u64::from(self.status_word),
                u64::from(native.status_word),
            ),
            (
                "x87_tag_word",
                u64::from(self.tag_word),
                u64::from(native.tag_word),
            ),
            ("x87_data_ptr", self.data_ptr, native.data_ptr),
            ("x87_instr_ptr", self.instr_ptr, native.instr_ptr),
            (
                "x87_last_opcode",
                u64::from(self.last_opcode),
                u64::from(native.last_opcode),
            ),
            ("x87_top", u64::from(self.top), u64::from(native.top)),
        ] {
            if interp != jit {
                diffs.push(format!("{name}: interp={interp:#x} jit={jit:#x}"));
            }
        }
        for index in 0..8 {
            let interp = self.st[index].to_bits();
            let jit = native.st[index].to_bits();
            if interp != jit {
                diffs.push(format!("st{index}: interp={interp:#x} jit={jit:#x}"));
            }
        }
    }
}
