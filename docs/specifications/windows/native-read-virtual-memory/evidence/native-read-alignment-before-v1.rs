use rax::user::windows::{
    WindowsConfig, WindowsProcess, dll,
    hle::{Ctx, Flow, Item, Value},
    memory::{Mem, mem, prot},
};
fn main() {
    let image = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let cfg = WindowsConfig::embedded("C:\\app\\alignment.exe", vec![], vec![], 4096).unwrap();
    let mut process = WindowsProcess::spawn_image(cfg, image).unwrap();
    let p = process.state_mut();
    let tid = *p.threads.keys().next().unwrap();
    let mut thread = p.threads.remove(&tid).unwrap();
    let mut bases = [0u64; 3];
    for base in &mut bases {
        *base =
            p.vm.allocate(None, 3 * 4096, mem::RESERVE | mem::COMMIT, prot::READWRITE)
                .unwrap()
                .0;
    }
    let source: Vec<u8> = (0..3 * 4096)
        .map(|i| (i as u8).wrapping_mul(17).wrapping_add(5))
        .collect();
    p.space.wr(bases[0], &source).unwrap();
    p.space.wr(bases[1], &vec![0xA5; 3 * 4096]).unwrap();
    p.space.wr(bases[2], &vec![0xD3; 3 * 4096]).unwrap();
    p.vm.protect(bases[1] + 8192, 4096, prot::NOACCESS).unwrap();
    let src = bases[0] + 4096 + 17;
    let dst = bases[1] + 8192 - 31;
    let returned = bases[2] + 4096 + 16;
    let api = dll::find("ntdll.dll")
        .unwrap()
        .exports
        .iter()
        .flat_map(|table| table.iter())
        .find_map(|export| match &export.item {
            Item::Func(api) if api.name == "NtReadVirtualMemory" => Some(api),
            _ => None,
        })
        .unwrap();
    for (i, value) in [u64::MAX, src, dst, 512, returned].into_iter().enumerate() {
        thread.cpu.set_gpr(i, value);
    }
    let sp = thread.cpu.sp();
    let mut context = Ctx {
        p,
        t: &mut thread,
        api,
        entry_pc: 0,
        entry_sp: sp,
        ret_addr: 0,
        cursor: sp,
    };
    let result = (api.imp)(&mut context).unwrap();
    let Flow::Ret(Value::Int(status)) = result else {
        panic!("unexpected flow");
    };
    let actual = context.mem().bytes(dst, 31).unwrap();
    let expected = vec![
        38, 55, 72, 89, 106, 123, 140, 157, 174, 191, 208, 225, 242, 3, 20, 37, 54, 71, 88, 105,
        122, 139, 156, 173, 190, 207, 224, 241, 2, 19, 36,
    ];
    println!(
        "status={status:#x} count={} actual={actual:X?} native={expected:X?}",
        context.mem().ptr(returned, 8).unwrap()
    );
    assert_eq!(status, 0x8000000D);
    assert_eq!(actual, expected);
}
