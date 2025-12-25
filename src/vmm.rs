use std::sync::Arc;

use tracing::{debug, info};

use crate::arch::{self, Arch, BootInfo};
#[cfg(all(feature = "kvm", target_os = "linux"))]
use crate::backend::kvm::KvmVm;
use crate::backend::{self, Vm};
#[cfg(all(feature = "kvm", target_os = "linux"))]
use crate::config::BackendKind;
use crate::config::VmConfig;
use crate::cpu::{CpuState, VCpu, VcpuExit};
use crate::devices::bus::{IoBus, IoDevice, IoRange, MmioBus, MmioRange};
use crate::devices::lapic::{LapicDevice, LocalApic, LAPIC_BASE, LAPIC_SIZE};
use crate::devices::pic::{DualPic, MasterPicDevice, SlavePicDevice};
use crate::devices::pit::Pit;
use crate::devices::serial::{Serial16550, SerialMmioDevice};
use crate::error::{Error, Result};
use crate::memory::GuestMemoryWrapper;

const SERIAL_BASE: u16 = 0x3f8;

/// Wrapper to make Pit implement IoDevice via shared reference
struct PitDevice {
    pit: Arc<std::sync::Mutex<Pit>>,
}

impl PitDevice {
    fn new(pit: Arc<std::sync::Mutex<Pit>>) -> Self {
        PitDevice { pit }
    }
}

impl IoDevice for PitDevice {
    fn read(&mut self, port: u16) -> u8 {
        if let Ok(mut pit) = self.pit.lock() {
            pit.read(port)
        } else {
            0xFF
        }
    }

    fn write(&mut self, port: u16, value: u8) {
        if let Ok(mut pit) = self.pit.lock() {
            pit.write(port, value);
        }
    }
}

pub struct Vmm {
    vm: Box<dyn Vm>,
    guest_mem: GuestMemoryWrapper,
    io_bus: IoBus,
    mmio_bus: MmioBus,
    serial: Arc<std::sync::Mutex<Serial16550>>,
    pit: Arc<std::sync::Mutex<Pit>>,
    pic: Arc<std::sync::Mutex<DualPic>>,
    lapic: Arc<std::sync::Mutex<LocalApic>>,
    vcpus: Vec<Box<dyn VCpu>>,
    arch: Box<dyn Arch>,
    boot_info: BootInfo,
    serial_mmio_base: Option<u64>,
    serial_irq: Option<u32>,
}

impl Vmm {
    pub fn new(config: VmConfig) -> Result<Self> {
        info!(
            vcpus = config.vcpus,
            mem_bytes = config.memory.bytes(),
            "initializing VMM"
        );

        // Create backend
        let backend = backend::create(&config)?;
        info!(backend = backend.name(), "using backend");

        // Create VM
        let vm = backend.create_vm()?;

        // Allocate guest memory
        let guest_mem = GuestMemoryWrapper::new(config.memory.bytes())?;

        // Register memory with VM (backend-specific)
        #[cfg(all(feature = "kvm", target_os = "linux"))]
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
        let mut mmio_bus = MmioBus::new();
        arch.setup_devices(&mut io_bus, &mut mmio_bus)?;

        let serial_mmio_base = arch.serial_mmio_base();
        let serial_irq = arch.serial_irq();

        // Create serial device with input enabled
        let serial = Arc::new(std::sync::Mutex::new(Serial16550::new(SERIAL_BASE)));
        if let Ok(mut serial_guard) = serial.lock() {
            if let Some(base) = serial_mmio_base {
                serial_guard.set_mmio_base(base);
            }
            serial_guard.enable_input();
        }
        if let Some(base) = serial_mmio_base {
            mmio_bus.register(
                MmioRange { base, len: 8 },
                Box::new(SerialMmioDevice::new(serial.clone())),
            )?;
        }

        // Create PIT (Programmable Interval Timer) at ports 0x40-0x43
        let pit = Arc::new(std::sync::Mutex::new(Pit::new()));

        // Create PIC (Programmable Interrupt Controller)
        let pic = Arc::new(std::sync::Mutex::new(DualPic::new()));

        // Register PIT on I/O bus
        io_bus.register(
            IoRange { base: 0x40, len: 4 },
            Box::new(PitDevice::new(pit.clone())),
        )?;

        // Register master PIC (0x20-0x21)
        io_bus.register(
            IoRange { base: 0x20, len: 2 },
            Box::new(MasterPicDevice::new(pic.clone())),
        )?;

        // Register slave PIC (0xA0-0xA1)
        io_bus.register(
            IoRange { base: 0xA0, len: 2 },
            Box::new(SlavePicDevice::new(pic.clone())),
        )?;

        // Create and register Local APIC at 0xFEE00000
        let lapic = Arc::new(std::sync::Mutex::new(LocalApic::new(0)));
        mmio_bus.register(
            MmioRange {
                base: LAPIC_BASE,
                len: LAPIC_SIZE,
            },
            Box::new(LapicDevice::new(lapic.clone())),
        )?;

        // Load kernel
        let boot_info = arch.load_kernel(guest_mem.memory(), &config)?;

        // Initialize VM (backend-specific)
        #[cfg(all(feature = "kvm", target_os = "linux"))]
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
            mmio_bus,
            serial,
            pit,
            pic,
            lapic,
            vcpus,
            arch,
            boot_info,
            serial_mmio_base,
            serial_irq,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        info!("starting vCPU 0");
        loop {
            // Poll for input before running vCPU
            if let Ok(mut serial) = self.serial.lock() {
                serial.poll_input();

                if serial.has_pending_interrupt() {
                    if let Some(irq) = self.serial_irq {
                        let _ = self.vm.set_irq_line(irq, true);
                        let _ = self.vm.set_irq_line(irq, false);
                    }
                }
            }

            // Tick the PIT and check for timer interrupts
            if let Ok(mut pit) = self.pit.lock() {
                if pit.tick() {
                    // Timer fired - raise IRQ 0 via the PIC
                    if let Ok(mut pic) = self.pic.lock() {
                        pic.set_irq(0, true);
                    }
                }
            }

            // Tick the LAPIC timer and check for timer interrupts
            {
                let vcpu = self
                    .vcpus
                    .get_mut(0)
                    .ok_or_else(|| Error::InvalidConfig("no vcpu available".to_string()))?;

                if let Ok(mut lapic) = self.lapic.lock() {
                    if let Some(vector) = lapic.tick() {
                        // LAPIC timer interrupt - inject directly if possible
                        if vcpu.can_inject_interrupt() {
                            if vcpu.inject_interrupt(vector).unwrap_or(false) {
                                lapic.clear_timer_pending();
                            }
                        }
                    } else if lapic.has_pending_timer() && vcpu.can_inject_interrupt() {
                        // Previously pending timer interrupt
                        let vector = lapic.timer_vector();
                        if vcpu.inject_interrupt(vector).unwrap_or(false) {
                            lapic.clear_timer_pending();
                        }
                    }
                }
            }

            // Check for pending PIC interrupts and inject them
            {
                let vcpu = self
                    .vcpus
                    .get_mut(0)
                    .ok_or_else(|| Error::InvalidConfig("no vcpu available".to_string()))?;

                if vcpu.can_inject_interrupt() {
                    if let Ok(mut pic) = self.pic.lock() {
                        if let Some(vector) = pic.get_pending_vector() {
                            let _ = vcpu.inject_interrupt(vector);
                        }
                    }
                }
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
                    match vcpu.get_state()? {
                        CpuState::X86_64(state) => {
                            let regs = state.regs;
                            let sregs = state.sregs;
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
                        }
                        CpuState::Hexagon(state) => {
                            let regs = state.regs;
                            let sp = regs.r[29];
                            info!(
                                pc = format!("{:#x}", regs.pc()),
                                sp = format!("{:#x}", sp),
                                usr = format!("{:#x}", regs.usr()),
                                "vCPU shutdown"
                            );
                        }
                    }
                    break;
                }
                VcpuExit::IoIn { port, size } => {
                    debug!(port = port, size = size, "PIO read");
                    let is_serial = port >= SERIAL_BASE && port < SERIAL_BASE + 8;
                    let mut data = vec![0u8; size as usize];
                    if is_serial {
                        if let Ok(mut serial) = self.serial.lock() {
                            for (i, byte) in data.iter_mut().enumerate() {
                                *byte = IoDevice::read(&mut *serial, port + i as u16);
                            }
                        }
                    } else {
                        self.io_bus.read(port, &mut data)?;
                    }
                    vcpu.complete_io_in(&data);
                }
                VcpuExit::IoOut { port, data } => {
                    debug!(port = port, size = data.len(), "PIO write");
                    let is_serial = port >= SERIAL_BASE && port < SERIAL_BASE + 8;
                    if is_serial {
                        if let Ok(mut serial) = self.serial.lock() {
                            for (i, byte) in data.iter().enumerate() {
                                IoDevice::write(&mut *serial, port + i as u16, *byte);
                            }
                            if port == SERIAL_BASE {
                                if let Some(irq) = self.serial_irq {
                                    let _ = self.vm.set_irq_line(irq, true);
                                    let _ = self.vm.set_irq_line(irq, false);
                                }
                            }
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
                VcpuExit::MmioRead { addr, size } => {
                    let mut data = vec![0u8; size as usize];
                    self.mmio_bus.read(addr, &mut data)?;
                    vcpu.complete_io_in(&data);
                }
                VcpuExit::MmioWrite { addr, data } => {
                    self.mmio_bus.write(addr, &data)?;
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
