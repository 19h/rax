use std::sync::Arc;

use tracing::{debug, info};

use crate::arch::{self, Arch, BootInfo};
#[cfg(feature = "kvm")]
use crate::backend::kvm::KvmVm;
use crate::backend::{self, Vm};
#[cfg(feature = "kvm")]
use crate::config::BackendKind;
use crate::config::VmConfig;
use crate::cpu::{VCpu, VcpuExit};
use crate::devices::bus::{IoBus, IoDevice};
use crate::devices::serial::Serial16550;
use crate::error::{Error, Result};
use crate::memory::GuestMemoryWrapper;

const SERIAL_BASE: u16 = 0x3f8;
const SERIAL_IRQ: u32 = 4;

pub struct Vmm {
    vm: Box<dyn Vm>,
    guest_mem: GuestMemoryWrapper,
    io_bus: IoBus,
    serial: Serial16550,
    vcpus: Vec<Box<dyn VCpu>>,
    arch: Box<dyn Arch>,
    boot_info: BootInfo,
}

impl Vmm {
    pub fn new(config: VmConfig) -> Result<Self> {
        info!(
            vcpus = config.vcpus,
            mem_bytes = config.memory.bytes(),
            "initializing VMM"
        );

        // Create backend
        let backend = backend::create(config.backend)?;
        info!(backend = backend.name(), "using backend");

        // Create VM
        let vm = backend.create_vm()?;

        // Allocate guest memory
        let guest_mem = GuestMemoryWrapper::new(config.memory.bytes())?;

        // Register memory with VM (backend-specific)
        #[cfg(feature = "kvm")]
        if matches!(config.backend, BackendKind::Kvm) {
            let kvm_vm = vm
                .as_any()
                .downcast_ref::<KvmVm>()
                .ok_or_else(|| Error::InvalidConfig("expected KVM VM".to_string()))?;
            guest_mem.register(kvm_vm.vm_fd())?;
        }
        // Emulator accesses memory directly, no registration needed

        // Setup architecture
        let arch = arch::from_kind(config.arch);
        info!(arch = arch.name(), "selected architecture");

        // Setup I/O devices
        let mut io_bus = IoBus::new();
        arch.setup_devices(&mut io_bus)?;

        // Create serial device with input enabled
        let mut serial = Serial16550::new(SERIAL_BASE);
        serial.enable_input();

        // Load kernel
        let boot_info = arch.load_kernel(guest_mem.memory(), &config)?;

        // Initialize VM (backend-specific)
        #[cfg(feature = "kvm")]
        if matches!(config.backend, BackendKind::Kvm) {
            let kvm_vm = vm
                .as_any()
                .downcast_ref::<KvmVm>()
                .ok_or_else(|| Error::InvalidConfig("expected KVM VM".to_string()))?;
            arch.init_vm(kvm_vm, &boot_info)?;
        }
        // Emulator doesn't need VM-level initialization

        // Create vCPUs
        let mem_arc = Arc::new(guest_mem.memory().clone());
        let mut vcpus = Vec::with_capacity(config.vcpus as usize);
        for cpu_id in 0..config.vcpus {
            let mut vcpu = vm.create_vcpu(cpu_id as u32, mem_arc.clone())?;

            // Setup initial CPU state for BSP (cpu 0)
            if cpu_id == 0 {
                let initial_state = arch.initial_cpu_state(guest_mem.memory(), &boot_info)?;
                vcpu.set_state(&initial_state)?;
            }

            debug!(vcpu_id = cpu_id, "created vCPU");
            vcpus.push(vcpu);
        }

        Ok(Vmm {
            vm,
            guest_mem,
            io_bus,
            serial,
            vcpus,
            arch,
            boot_info,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        info!("starting vCPU 0");
        loop {
            // Poll for input before running vCPU
            self.serial.poll_input();

            // If there's pending input and interrupts are enabled, inject IRQ
            if self.serial.has_pending_interrupt() {
                let _ = self.vm.set_irq_line(SERIAL_IRQ, true);
                let _ = self.vm.set_irq_line(SERIAL_IRQ, false);
            }

            let vcpu = self
                .vcpus
                .get_mut(0)
                .ok_or_else(|| Error::InvalidConfig("no vcpu available".to_string()))?;

            match vcpu.run()? {
                VcpuExit::Hlt => {
                    // HLT is normal - kernel waits for interrupts
                    continue;
                }
                VcpuExit::Shutdown => {
                    let regs = vcpu.get_regs()?;
                    let sregs = vcpu.get_sregs()?;
                    info!(
                        rip = format!("{:#x}", regs.rip),
                        rsp = format!("{:#x}", regs.rsp),
                        rsi = format!("{:#x}", regs.rsi),
                        rflags = format!("{:#x}", regs.rflags),
                        cr0 = format!("{:#x}", sregs.cr0),
                        cr3 = format!("{:#x}", sregs.cr3),
                        cr4 = format!("{:#x}", sregs.cr4),
                        cs_sel = format!("{:#x}", sregs.cs.selector),
                        cs_base = format!("{:#x}", sregs.cs.base),
                        ds_sel = format!("{:#x}", sregs.ds.selector),
                        gdt_base = format!("{:#x}", sregs.gdt.base),
                        gdt_limit = format!("{:#x}", sregs.gdt.limit),
                        "vCPU shutdown"
                    );
                    break;
                }
                VcpuExit::IoIn { port, size } => {
                    debug!(port = port, size = size, "PIO read");
                    let is_serial = port >= SERIAL_BASE && port < SERIAL_BASE + 8;
                    let mut data = vec![0u8; size as usize];
                    if is_serial {
                        for (i, byte) in data.iter_mut().enumerate() {
                            *byte = self.serial.read(port + i as u16);
                        }
                    } else {
                        self.io_bus.read(port, &mut data)?;
                    }
                    vcpu.complete_io_in(&data);
                }
                VcpuExit::IoOut { port, data } => {
                    use std::sync::atomic::{AtomicU64, Ordering};
                    static IO_COUNT: AtomicU64 = AtomicU64::new(0);
                    let count = IO_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
                    if count <= 30 {
                        eprintln!("[IO] OUT port={:#x} data={:02x?}", port, data);
                    }

                    debug!(port = port, size = data.len(), "PIO write");
                    let is_serial = port >= SERIAL_BASE && port < SERIAL_BASE + 8;
                    if is_serial {
                        for (i, byte) in data.iter().enumerate() {
                            self.serial.write(port + i as u16, *byte);
                        }
                        // Inject serial interrupt after data writes
                        if port == SERIAL_BASE {
                            let _ = self.vm.set_irq_line(SERIAL_IRQ, true);
                            let _ = self.vm.set_irq_line(SERIAL_IRQ, false);
                        }
                    } else if port == 0xE9 {
                        // Bochs debug port - output directly
                        for byte in &data {
                            eprint!("{}", *byte as char);
                        }
                    } else {
                        self.io_bus.write(port, &data)?;
                    }
                }
                VcpuExit::SystemEvent { .. } => break,
                VcpuExit::FailEntry { reason } => {
                    return Err(Error::KernelLoad(format!(
                        "vCPU fail entry: reason={reason:#x}"
                    )))
                }
                VcpuExit::InternalError => {
                    return Err(Error::KernelLoad("vCPU internal error".to_string()))
                }
                exit => {
                    return Err(Error::KernelLoad(format!("unhandled exit: {exit:?}")))
                }
            }
        }
        Ok(())
    }

    pub fn boot_info(&self) -> &BootInfo {
        &self.boot_info
    }

    pub fn guest_mem(&self) -> &GuestMemoryWrapper {
        &self.guest_mem
    }

    pub fn arch(&self) -> &dyn Arch {
        self.arch.as_ref()
    }
}
