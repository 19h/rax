fn main() {
    let mut d = rax::arm::decoder::Decoder::new(rax::arm::ExecutionState::Aarch32);
    d.set_state(rax::arm::ExecutionState::Aarch32);
    for raw in [0xe321f0d2u32, 0xe321f0d3, 0xe10f0000, 0xe129f000] {
        let b = raw.to_le_bytes();
        match d.decode(&b) {
            Ok(i) => println!("{raw:#010x} -> {:?}", i.mnemonic),
            Err(e) => println!("{raw:#010x} -> ERR {e:?}"),
        }
    }
}
