//! Helper-backed scalar MOVBE lowering and native execution tests.

use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::OpId;
use crate::smir::ir::{SmirBlock, SmirFunction};

const PC: u64 = 0x1000;

fn x86_gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn function(load: bool, width: OpWidth, register_index: u8) -> SmirFunction {
    let temporary = VReg::Virtual(crate::smir::ir::types::VirtualId(7));
    let register = x86_gpr(register_index);
    let address = Address::Direct(x86_gpr(3));
    let mut block = SmirBlock::new(crate::smir::ir::types::BlockId(0), PC);
    if load {
        block.ops.push(SmirOp::new(
            OpId(0),
            PC,
            OpKind::Load {
                dst: temporary,
                addr: address,
                width: width.to_mem_width(),
                sign: SignExtend::Zero,
            },
        ));
        block.ops.push(SmirOp::new(
            OpId(1),
            PC,
            OpKind::Bswap {
                dst: register,
                src: temporary,
                width,
            },
        ));
    } else {
        block.ops.push(SmirOp::new(
            OpId(0),
            PC,
            OpKind::Bswap {
                dst: temporary,
                src: register,
                width,
            },
        ));
        block.ops.push(SmirOp::new(
            OpId(1),
            PC,
            OpKind::Store {
                src: temporary,
                addr: address,
                width: width.to_mem_width(),
            },
        ));
    }
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn lower(
    load: bool,
    width: OpWidth,
    register_index: u8,
    helpers: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(helpers);
    let result = lowerer.lower_function(&function(load, width, register_index))?;
    assert!(result.relocations.is_empty());
    Ok((lowerer.finalize()?, result.entry_offset))
}

#[test]
fn movbe_memory_fusion_lowers_every_width_direction_and_register_class() {
    for load in [false, true] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for register_index in [0, 4, 5, 16, 31] {
                lower(load, width, register_index, true).unwrap_or_else(|error| {
                    panic!("load={load} width={width:?} register={register_index}: {error:?}")
                });
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Default)]
struct MemoryContext {
    load_value: u64,
    load_ok: u64,
    store_ok: u64,
    last_addr: u64,
    last_size: u64,
    last_value: u64,
    loads: u64,
    stores: u64,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn load_helper(
    context: *mut MemoryContext,
    addr: u64,
    size: u64,
    _signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.last_addr = addr;
    context.last_size = size;
    LoadResult {
        value: context.load_value,
        ok: context.load_ok,
    }
}

#[cfg(target_arch = "x86_64")]
extern "C" fn store_helper(context: *mut MemoryContext, addr: u64, value: u64, size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_value = value;
    context.store_ok
}

fn swapped(value: u64, width: OpWidth) -> u64 {
    match width {
        OpWidth::W16 => u64::from((value as u16).swap_bytes()),
        OpWidth::W32 => u64::from((value as u32).swap_bytes()),
        OpWidth::W64 => value.swap_bytes(),
        _ => unreachable!(),
    }
}

fn merged(old: u64, value: u64, width: OpWidth) -> u64 {
    match width {
        OpWidth::W16 => (old & !0xFFFF) | (value & 0xFFFF),
        OpWidth::W32 => value as u32 as u64,
        OpWidth::W64 => value,
        _ => unreachable!(),
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_movbe_memory_is_exact_flag_neutral_and_fault_noncommitting_for_all_gpr_classes() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const VALUE: u64 = 0x0123_4567_89AB_CDEF;
    const ADDRESS: u64 = 0x4000;
    const FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);
    const FLAG_MASK: u64 = 0x08D5 | (1 << 10);
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;

    let mut initial_gprs = [0_u64; 32];
    for (index, value) in initial_gprs.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    initial_gprs[3] = ADDRESS;

    for register_index in [0_usize, 4, 5, 16, 31] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let (load_code, load_entry) = lower(true, width, register_index as u8, true).unwrap();
            let load_exec = ExecMem::new(&load_code).expect("map MOVBE load");
            for load_ok in [0, 1] {
                let mut context = MemoryContext {
                    load_value: VALUE,
                    load_ok,
                    ..MemoryContext::default()
                };
                let mut registers = GuestRegs::default();
                registers.gpr = initial_gprs;
                registers.rflags = FLAGS;
                registers.exit_pc = SENTINEL_PC;
                registers.ctx = (&mut context as *mut MemoryContext) as u64;
                registers.load_fn = load_helper as usize as u64;
                load_exec.run(load_entry, &mut registers);

                let mut expected = initial_gprs;
                if load_ok != 0 {
                    expected[register_index] =
                        merged(initial_gprs[register_index], swapped(VALUE, width), width);
                }
                assert_eq!(
                    registers.gpr, expected,
                    "load W={width:?} R{register_index}"
                );
                assert_eq!(registers.rflags & FLAG_MASK, FLAGS & FLAG_MASK);
                assert_eq!(
                    registers.exit_pc,
                    if load_ok != 0 { SENTINEL_PC } else { PC }
                );
                assert_eq!((context.loads, context.last_addr), (1, ADDRESS));
                assert_eq!(context.last_size, width.bytes() as u64);
            }

            let (store_code, store_entry) =
                lower(false, width, register_index as u8, true).unwrap();
            let store_exec = ExecMem::new(&store_code).expect("map MOVBE store");
            for store_ok in [0, 1] {
                let mut context = MemoryContext {
                    store_ok,
                    ..MemoryContext::default()
                };
                let mut registers = GuestRegs::default();
                registers.gpr = initial_gprs;
                registers.rflags = FLAGS;
                registers.exit_pc = SENTINEL_PC;
                registers.ctx = (&mut context as *mut MemoryContext) as u64;
                registers.store_fn = store_helper as usize as u64;
                store_exec.run(store_entry, &mut registers);

                assert_eq!(
                    registers.gpr, initial_gprs,
                    "store W={width:?} R{register_index}"
                );
                assert_eq!(registers.rflags & FLAG_MASK, FLAGS & FLAG_MASK);
                assert_eq!(
                    registers.exit_pc,
                    if store_ok != 0 { SENTINEL_PC } else { PC }
                );
                assert_eq!((context.stores, context.last_addr), (1, ADDRESS));
                assert_eq!(context.last_size, width.bytes() as u64);
                let mask = match width {
                    OpWidth::W16 => 0xFFFF,
                    OpWidth::W32 => 0xFFFF_FFFF,
                    OpWidth::W64 => u64::MAX,
                    _ => unreachable!(),
                };
                assert_eq!(
                    context.last_value & mask,
                    swapped(initial_gprs[register_index], width) & mask,
                    "store value W={width:?} R{register_index}"
                );
            }
        }
    }
}
