//! V68 tables 10-4/10-5: the first ICLASS column is low Slot 0.
use rax::config::{Endianness, HexagonIsa};
use rax::isa::hexagon::HexagonVcpu;
use rax::isa::hexagon::decode::{AddrMode, DecodedInsn, MemSign, MemWidth, decode_duplex};
use rax::vm::vcpu::{CpuState, HexagonRegisters, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

#[derive(Clone, Copy, Debug)]
enum Action {
    LoadWord(u8, u8),
    LoadHalf(u8, u8),
    StoreWord(u8, u8),
    StoreHalf(u8, u8),
    Increment(u8),
}
use Action::*;

impl Action {
    fn opcode(self) -> &'static str {
        match self {
            LoadWord(..) => "SL1_loadri_io",
            LoadHalf(..) => "SL2_loadrh_io",
            StoreWord(..) => "SS1_storew_io",
            StoreHalf(..) => "SS2_storeh_io",
            Increment(..) => "SA1_inc",
        }
    }

    fn check_decode(self, instruction: &DecodedInsn) {
        let matches = match (self, instruction) {
            (
                LoadWord(d, b),
                DecodedInsn::Load {
                    dst,
                    addr: AddrMode::Offset { base, offset: 0 },
                    width: MemWidth::Word,
                    sign: MemSign::Unsigned,
                    pred: None,
                },
            )
            | (
                LoadHalf(d, b),
                DecodedInsn::Load {
                    dst,
                    addr: AddrMode::Offset { base, offset: 0 },
                    width: MemWidth::Half,
                    sign: MemSign::Signed,
                    pred: None,
                },
            ) => *dst == d && *base == b,
            (
                StoreWord(s, b),
                DecodedInsn::Store {
                    src,
                    addr: AddrMode::Offset { base, offset: 0 },
                    width: MemWidth::Word,
                    pred: None,
                    src_new: false,
                    high_half: false,
                },
            )
            | (
                StoreHalf(s, b),
                DecodedInsn::Store {
                    src,
                    addr: AddrMode::Offset { base, offset: 0 },
                    width: MemWidth::Half,
                    pred: None,
                    src_new: false,
                    high_half: false,
                },
            ) => *src == s && *base == b,
            (Increment(r), DecodedInsn::AddImm { dst, src, imm: 1 }) => *dst == r && *src == r,
            _ => false,
        };
        assert!(matches, "expected {self:?}, decoded {instruction:?}");
    }

    fn expected(
        self,
        old: &HexagonRegisters,
        registers: &mut HexagonRegisters,
        memory: &mut [u8; 32],
        original: &[u8; 32],
        endian: Endianness,
    ) {
        match self {
            LoadWord(dst, base) | LoadHalf(dst, base) => {
                let index = (old.r[base as usize] - 0x2000) as usize;
                let value = if matches!(self, LoadWord(..)) {
                    let bytes = original[index..index + 4].try_into().unwrap();
                    match endian {
                        Endianness::Little => u32::from_le_bytes(bytes),
                        Endianness::Big => u32::from_be_bytes(bytes),
                    }
                } else {
                    let bytes = original[index..index + 2].try_into().unwrap();
                    let half = match endian {
                        Endianness::Little => i16::from_le_bytes(bytes),
                        Endianness::Big => i16::from_be_bytes(bytes),
                    };
                    half as i32 as u32
                };
                registers.r[dst as usize] = value;
            }
            StoreWord(src, base) | StoreHalf(src, base) => {
                let index = (old.r[base as usize] - 0x2000) as usize;
                let value = old.r[src as usize];
                if matches!(self, StoreWord(..)) {
                    let bytes = match endian {
                        Endianness::Little => value.to_le_bytes(),
                        Endianness::Big => value.to_be_bytes(),
                    };
                    memory[index..index + 4].copy_from_slice(&bytes);
                } else {
                    let bytes = match endian {
                        Endianness::Little => (value as u16).to_le_bytes(),
                        Endianness::Big => (value as u16).to_be_bytes(),
                    };
                    memory[index..index + 2].copy_from_slice(&bytes);
                }
            }
            Increment(register) => {
                registers.r[register as usize] = old.r[register as usize].wrapping_add(1)
            }
        }
    }
}

fn contract(word: u32, class: u8, low: Action, high: Action) {
    assert_eq!(
        (((word >> 29) & 7) << 1) | ((word >> 13) & 1),
        u32::from(class)
    );
    let (slot1, slot0) = decode_duplex(word, HexagonIsa::V68).unwrap();
    low.check_decode(&slot0.insn);
    high.check_decode(&slot1.insn);
    for endian in [Endianness::Little, Endianness::Big] {
        let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x4000)]).unwrap());
        let code = match endian {
            Endianness::Little => word.to_le_bytes(),
            Endianness::Big => word.to_be_bytes(),
        };
        memory.write_slice(&code, GuestAddress(0x1000)).unwrap();
        let options = rax::oracle::OracleOptions {
            isa: rax::oracle::OracleIsa::Hexagon,
            pc: 0x1000,
            hexagon_isa: HexagonIsa::V68,
            hexagon_endian: endian,
            ..Default::default()
        };
        let decoded = rax::oracle::decode_to_json(&code, &options).unwrap();
        assert_eq!(decoded["decoded_ops"][0]["duplex_slot"], 1);
        assert_eq!(decoded["decoded_ops"][0]["opcode"], high.opcode());
        assert_eq!(decoded["decoded_ops"][1]["duplex_slot"], 0);
        assert_eq!(decoded["decoded_ops"][1]["opcode"], low.opcode());
        // Availability identifies the lifter; a rejected instruction carries
        // an explicit error and no emitted operations in the existing schema.
        assert_eq!(decoded["smir"]["available"], true);
        assert!(
            decoded["smir"]["error"]
                .as_str()
                .unwrap()
                .contains("Hexagon duplex container")
        );
        assert!(decoded["smir"].get("ops").is_none());

        let mut bytes = [0xa5; 32];
        for (index, value) in [(0, 0x1122eeff_u32), (16, 0x88775566)] {
            let data = match endian {
                Endianness::Little => value.to_le_bytes(),
                Endianness::Big => value.to_be_bytes(),
            };
            bytes[index..index + 4].copy_from_slice(&data);
        }
        memory.write_slice(&bytes, GuestAddress(0x2000)).unwrap();
        let mut old = HexagonRegisters::default();
        old.set_pc(0x1000);
        old.r[1] = 0x2000;
        old.r[2] = 0xaabbccdd;
        old.r[3] = 0x12345678;
        old.r[4] = 0x2010;
        let mut expected = old.clone();
        let mut expected_memory = bytes;
        high.expected(&old, &mut expected, &mut expected_memory, &bytes, endian);
        low.expected(&old, &mut expected, &mut expected_memory, &bytes, endian);
        let mut cpu = HexagonVcpu::new(0, memory.clone(), HexagonIsa::V68, endian);
        cpu.set_state(&CpuState::hexagon(old)).unwrap();
        assert!(cpu.step_insn().unwrap().is_none());
        let CpuState::Hexagon(state) = cpu.get_state().unwrap() else {
            unreachable!()
        };
        assert_eq!(state.regs.r, expected.r, "class {class:x}, {endian:?}");
        assert_eq!(cpu.current_pc(), 0x1004);
        assert_eq!(cpu.instruction_count(), 1);
        let mut after = [0; 32];
        memory.read_slice(&mut after, GuestAddress(0x2000)).unwrap();
        assert_eq!(after, expected_memory, "class {class:x}, {endian:?}");
    }
}

// Fixed LLVM 23 V68 encodings; class B was checked with llvm-mc --disassemble.
// The action pairs follow the manual's low/high columns independently of the
// decoder's dispatch table. The matrix includes all 15 allocated classes.
macro_rules! case {
    ($name:ident, $word:expr, $class:expr, $low:expr, $high:expr) => {
        #[test]
        fn $name() {
            contract($word, $class, $low, $high);
        }
    };
}
case!(class_0, 0x00120043, 0x0, LoadWord(3, 4), LoadWord(2, 1));
case!(class_1, 0x00432012, 0x1, LoadHalf(2, 1), LoadWord(3, 4));
case!(class_2, 0x20120043, 0x2, LoadHalf(3, 4), LoadHalf(2, 1));
case!(class_3, 0x31223133, 0x3, Increment(3), Increment(2));
case!(class_4, 0x51330012, 0x4, LoadWord(2, 1), Increment(3));
case!(class_5, 0x51332012, 0x5, LoadHalf(2, 1), Increment(3));
case!(class_6, 0x71330012, 0x6, StoreWord(2, 1), Increment(3));
case!(class_7, 0x71332012, 0x7, StoreHalf(2, 1), Increment(3));
case!(class_8, 0x80430012, 0x8, StoreWord(2, 1), LoadWord(3, 4));
case!(class_9, 0x80432012, 0x9, StoreWord(2, 1), LoadHalf(3, 4));
case!(class_a, 0xa0120043, 0xa, StoreWord(3, 4), StoreWord(2, 1));
case!(class_b, 0xa0432012, 0xb, StoreHalf(2, 1), StoreWord(3, 4));
case!(class_c, 0xc0430012, 0xc, StoreHalf(2, 1), LoadWord(3, 4));
case!(class_d, 0xc0432012, 0xd, StoreHalf(2, 1), LoadHalf(3, 4));
case!(class_e, 0xe0120043, 0xe, StoreHalf(3, 4), StoreHalf(2, 1));

#[test]
fn reserved_class_f_is_rejected() {
    assert!(decode_duplex(0xe0432012, HexagonIsa::V68).is_none());
}

#[test]
fn ordinary_oracle_lifting_uses_the_same_word_in_both_guest_byte_orders() {
    use rax::oracle::{OracleIsa, OracleOptions, decode_to_json};
    let word = 0x9181c003_u32;
    let mut options = OracleOptions {
        isa: OracleIsa::Hexagon,
        pc: 0x1000,
        ..Default::default()
    };
    let little = decode_to_json(&word.to_le_bytes(), &options).unwrap();
    options.hexagon_endian = Endianness::Big;
    let big = decode_to_json(&word.to_be_bytes(), &options).unwrap();
    assert_eq!(little["smir"]["available"], true);
    assert_eq!(big["smir"], little["smir"]);
    options.include_smir = false;
    assert_eq!(
        decode_to_json(&word.to_be_bytes(), &options).unwrap()["smir"]["reason"],
        "disabled"
    );
}
