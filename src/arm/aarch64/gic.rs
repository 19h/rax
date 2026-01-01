//! GICv3 Generic Interrupt Controller
//!
//! This module implements the ARM GICv3 interrupt controller including:
//! - Distributor (GICD)
//! - Redistributor (GICR)
//! - CPU Interface (via system registers)
//! - SGI, PPI, and SPI support
//! - Interrupt prioritization and routing

use std::collections::VecDeque;

// =============================================================================
// GIC Constants
// =============================================================================

/// Number of Software Generated Interrupts (SGI).
pub const NUM_SGI: usize = 16;

/// Number of Private Peripheral Interrupts (PPI).
pub const NUM_PPI: usize = 16;

/// Number of Shared Peripheral Interrupts (SPI).
pub const MAX_SPI: usize = 988;

/// Total number of interrupt IDs (0-1019).
pub const MAX_INTID: usize = 1020;

/// SGI range: 0-15.
pub const SGI_START: u32 = 0;
pub const SGI_END: u32 = 15;

/// PPI range: 16-31.
pub const PPI_START: u32 = 16;
pub const PPI_END: u32 = 31;

/// SPI range: 32-1019.
pub const SPI_START: u32 = 32;
pub const SPI_END: u32 = 1019;

/// Special interrupt IDs.
pub const INTID_SPURIOUS: u32 = 1023;

// =============================================================================
// GIC Version
// =============================================================================

/// GIC version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GicVersion {
    /// GICv2.
    V2,
    /// GICv3.
    V3,
    /// GICv4.
    V4,
}

// =============================================================================
// Interrupt State
// =============================================================================

/// State of an interrupt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptState {
    /// Inactive - not pending and not active.
    Inactive,
    /// Pending - waiting to be serviced.
    Pending,
    /// Active - being serviced.
    Active,
    /// Active and Pending - being serviced and pending again.
    ActivePending,
}

impl InterruptState {
    /// Check if interrupt is pending.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending | Self::ActivePending)
    }

    /// Check if interrupt is active.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active | Self::ActivePending)
    }

    /// Set pending.
    pub fn set_pending(&mut self) {
        *self = match *self {
            Self::Inactive => Self::Pending,
            Self::Pending => Self::Pending,
            Self::Active => Self::ActivePending,
            Self::ActivePending => Self::ActivePending,
        };
    }

    /// Clear pending.
    pub fn clear_pending(&mut self) {
        *self = match *self {
            Self::Inactive => Self::Inactive,
            Self::Pending => Self::Inactive,
            Self::Active => Self::Active,
            Self::ActivePending => Self::Active,
        };
    }

    /// Set active.
    pub fn set_active(&mut self) {
        *self = match *self {
            Self::Inactive => Self::Active,
            Self::Pending => Self::Active,
            Self::Active => Self::Active,
            Self::ActivePending => Self::ActivePending,
        };
    }

    /// Clear active.
    pub fn clear_active(&mut self) {
        *self = match *self {
            Self::Inactive => Self::Inactive,
            Self::Pending => Self::Pending,
            Self::Active => Self::Inactive,
            Self::ActivePending => Self::Pending,
        };
    }
}

// =============================================================================
// Interrupt Configuration
// =============================================================================

/// Interrupt trigger type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerType {
    /// Level-sensitive.
    Level,
    /// Edge-triggered.
    Edge,
}

/// Interrupt routing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingMode {
    /// Route to specific affinity.
    Affinity(u64),
    /// Route to any available PE.
    Any,
}

// =============================================================================
// Interrupt Descriptor
// =============================================================================

/// Per-interrupt configuration.
#[derive(Clone, Debug)]
pub struct InterruptConfig {
    /// Interrupt state.
    pub state: InterruptState,
    /// Enabled.
    pub enabled: bool,
    /// Priority (0-255, lower = higher priority).
    pub priority: u8,
    /// Target CPU (for SPIs).
    pub targets: u8,
    /// Trigger type.
    pub trigger: TriggerType,
    /// Group (0 or 1).
    pub group: u8,
    /// Routing mode.
    pub routing: RoutingMode,
    /// Non-secure.
    pub non_secure: bool,
}

impl Default for InterruptConfig {
    fn default() -> Self {
        Self {
            state: InterruptState::Inactive,
            enabled: false,
            priority: 0xA0, // Default priority
            targets: 0x01,  // CPU 0
            trigger: TriggerType::Level,
            group: 1,
            routing: RoutingMode::Affinity(0),
            non_secure: true,
        }
    }
}

// =============================================================================
// GIC Configuration
// =============================================================================

/// GIC configuration.
#[derive(Clone, Debug)]
pub struct GicConfig {
    /// GIC version.
    pub version: GicVersion,
    /// Number of CPUs.
    pub num_cpus: u8,
    /// Number of SPIs.
    pub num_spis: u16,
    /// Base address of distributor.
    pub dist_base: u64,
    /// Base address of redistributor.
    pub redist_base: u64,
    /// Stride between redistributor regions.
    pub redist_stride: u64,
}

impl Default for GicConfig {
    fn default() -> Self {
        Self {
            version: GicVersion::V3,
            num_cpus: 1,
            num_spis: 256,
            dist_base: 0x0800_0000,
            redist_base: 0x080A_0000,
            redist_stride: 0x2_0000,
        }
    }
}

// =============================================================================
// CPU Interface (per-CPU state)
// =============================================================================

/// Per-CPU GIC state.
#[derive(Clone, Debug)]
pub struct CpuInterface {
    /// CPU ID.
    pub cpu_id: u8,
    /// Affinity (MPIDR-like).
    pub affinity: u64,
    /// Enable Group 0.
    pub enable_grp0: bool,
    /// Enable Group 1.
    pub enable_grp1: bool,
    /// Enable Group 1 (Non-secure).
    pub enable_grp1_ns: bool,
    /// Priority mask (interrupts >= this are masked).
    pub priority_mask: u8,
    /// Binary point for group 0.
    pub bpr0: u8,
    /// Binary point for group 1.
    pub bpr1: u8,
    /// End of Interrupt mode (0 = ICC_EOIR writes deactivate).
    pub eoi_mode: bool,
    /// Currently running priority.
    pub running_priority: u8,
    /// Active priorities (stack).
    pub active_priorities: Vec<(u32, u8)>, // (INTID, priority)
    /// Highest pending priority.
    pub highest_pending_priority: u8,
    /// Highest pending interrupt ID.
    pub highest_pending_intid: u32,
    /// SGI/PPI state (per-CPU).
    pub private_interrupts: [InterruptConfig; NUM_SGI + NUM_PPI],
    /// Pending LPIs.
    pub pending_lpis: VecDeque<u32>,
    /// ICC_SRE_EL1 (System Register Enable).
    pub sre_el1: u64,
    /// ICC_SRE_EL2.
    pub sre_el2: u64,
    /// ICC_SRE_EL3.
    pub sre_el3: u64,
    /// ICC_CTLR_EL1.
    pub ctlr_el1: u64,
    /// ICC_CTLR_EL3.
    pub ctlr_el3: u64,
    /// ICC_IGRPEN0_EL1.
    pub igrpen0: u64,
    /// ICC_IGRPEN1_EL1.
    pub igrpen1: u64,
    /// ICC_IGRPEN1_EL3.
    pub igrpen1_el3: u64,
}

impl CpuInterface {
    /// Create a new CPU interface.
    pub fn new(cpu_id: u8) -> Self {
        let mut private_interrupts: [InterruptConfig; NUM_SGI + NUM_PPI] =
            std::array::from_fn(|_| InterruptConfig::default());

        // SGIs are edge-triggered
        for config in private_interrupts.iter_mut().take(NUM_SGI) {
            config.trigger = TriggerType::Edge;
        }

        Self {
            cpu_id,
            affinity: cpu_id as u64,
            enable_grp0: false,
            enable_grp1: false,
            enable_grp1_ns: false,
            priority_mask: 0xFF,
            bpr0: 0,
            bpr1: 0,
            eoi_mode: false,
            running_priority: 0xFF,
            active_priorities: Vec::new(),
            highest_pending_priority: 0xFF,
            highest_pending_intid: INTID_SPURIOUS,
            private_interrupts,
            pending_lpis: VecDeque::new(),
            sre_el1: 0x7, // System registers enabled
            sre_el2: 0x7,
            sre_el3: 0x7,
            ctlr_el1: 0,
            ctlr_el3: 0,
            igrpen0: 0,
            igrpen1: 0,
            igrpen1_el3: 0,
        }
    }

    /// Update highest pending interrupt.
    pub fn update_pending(&mut self, spis: &[InterruptConfig]) {
        let mut best_priority = 0xFF;
        let mut best_intid = INTID_SPURIOUS;

        // Check private interrupts (SGI/PPI)
        for (i, config) in self.private_interrupts.iter().enumerate() {
            if config.enabled && config.state.is_pending() && config.priority < best_priority {
                // Check if unmasked
                if config.priority < self.priority_mask {
                    best_priority = config.priority;
                    best_intid = i as u32;
                }
            }
        }

        // Check SPIs
        for (i, config) in spis.iter().enumerate() {
            let intid = SPI_START + i as u32;
            if config.enabled && config.state.is_pending() && config.priority < best_priority {
                // Check routing
                let routed = match config.routing {
                    RoutingMode::Any => true,
                    RoutingMode::Affinity(aff) => aff == self.affinity,
                };

                if routed && config.priority < self.priority_mask {
                    best_priority = config.priority;
                    best_intid = intid;
                }
            }
        }

        self.highest_pending_priority = best_priority;
        self.highest_pending_intid = best_intid;
    }

    /// Check if there's a pending interrupt to signal.
    pub fn pending_interrupt(&self) -> bool {
        // Check if any group is enabled
        let grp0_enabled = self.enable_grp0;
        let grp1_enabled = self.enable_grp1 || self.enable_grp1_ns;

        if !grp0_enabled && !grp1_enabled {
            return false;
        }

        self.highest_pending_intid != INTID_SPURIOUS
            && self.highest_pending_priority < self.running_priority
    }

    /// Acknowledge interrupt (read ICC_IAR).
    pub fn acknowledge(&mut self, spis: &mut [InterruptConfig]) -> u32 {
        let intid = self.highest_pending_intid;

        if intid == INTID_SPURIOUS {
            return INTID_SPURIOUS;
        }

        // Get interrupt config
        let config = if intid < SPI_START {
            &mut self.private_interrupts[intid as usize]
        } else {
            &mut spis[(intid - SPI_START) as usize]
        };

        // Transition state
        // For level-triggered: Pending -> ActivePending (level still asserted)
        // For edge-triggered: Pending -> Active (edge consumed)
        if config.trigger == TriggerType::Level {
            // For level-triggered, keep pending state - become ActivePending
            config.state = match config.state {
                InterruptState::Pending => InterruptState::ActivePending,
                InterruptState::ActivePending => InterruptState::ActivePending,
                other => {
                    // Should not acknowledge non-pending interrupt, but handle gracefully
                    let mut s = other;
                    s.set_active();
                    s
                }
            };
        } else {
            // For edge-triggered, consume the pending - become Active
            config.state.set_active();
            config.state.clear_pending();
        }

        // Push to active stack
        self.active_priorities.push((intid, config.priority));
        self.running_priority = config.priority;

        // Update pending
        self.update_pending(spis);

        intid
    }

    /// End of Interrupt (write ICC_EOIR).
    pub fn end_of_interrupt(&mut self, intid: u32, spis: &mut [InterruptConfig]) {
        if intid == INTID_SPURIOUS {
            return;
        }

        // Find in active stack
        if let Some(pos) = self
            .active_priorities
            .iter()
            .position(|(id, _)| *id == intid)
        {
            self.active_priorities.remove(pos);
        }

        // Update running priority
        self.running_priority = self
            .active_priorities
            .last()
            .map(|(_, p)| *p)
            .unwrap_or(0xFF);

        // Deactivate interrupt (if not in EOI mode 1)
        if !self.eoi_mode {
            let config = if intid < SPI_START {
                &mut self.private_interrupts[intid as usize]
            } else {
                &mut spis[(intid - SPI_START) as usize]
            };
            config.state.clear_active();
        }

        self.update_pending(spis);
    }

    /// Deactivate interrupt (write ICC_DIR).
    pub fn deactivate(&mut self, intid: u32, spis: &mut [InterruptConfig]) {
        if intid == INTID_SPURIOUS {
            return;
        }

        let config = if intid < SPI_START {
            &mut self.private_interrupts[intid as usize]
        } else {
            &mut spis[(intid - SPI_START) as usize]
        };

        config.state.clear_active();
        self.update_pending(spis);
    }

    /// Send SGI.
    pub fn send_sgi(&mut self, intid: u32) {
        if intid < NUM_SGI as u32 {
            self.private_interrupts[intid as usize].state.set_pending();
        }
    }
}

// =============================================================================
// GIC (Distributor + Redistributors + CPU Interfaces)
// =============================================================================

/// ARM GICv3 Generic Interrupt Controller.
pub struct Gic {
    /// Configuration.
    config: GicConfig,
    /// Distributor enabled.
    dist_enabled: bool,
    /// Security state.
    security_enabled: bool,
    /// Affinity Routing Enable (ARE).
    are_s: bool, // Secure
    are_ns: bool, // Non-secure
    /// SPI configuration.
    spis: Vec<InterruptConfig>,
    /// Per-CPU interfaces.
    cpu_interfaces: Vec<CpuInterface>,
}

impl Gic {
    /// Create a new GIC.
    pub fn new(config: GicConfig) -> Self {
        let num_spis = config.num_spis as usize;
        let num_cpus = config.num_cpus as usize;

        let mut spis = Vec::with_capacity(num_spis);
        for _ in 0..num_spis {
            spis.push(InterruptConfig::default());
        }

        let mut cpu_interfaces = Vec::with_capacity(num_cpus);
        for i in 0..num_cpus {
            cpu_interfaces.push(CpuInterface::new(i as u8));
        }

        Self {
            config,
            dist_enabled: false,
            security_enabled: false,
            are_s: true,
            are_ns: true,
            spis,
            cpu_interfaces,
        }
    }

    /// Get GIC configuration.
    pub fn config(&self) -> &GicConfig {
        &self.config
    }

    /// Reset the GIC.
    pub fn reset(&mut self) {
        self.dist_enabled = false;
        self.security_enabled = false;
        self.are_s = true;
        self.are_ns = true;

        for spi in &mut self.spis {
            *spi = InterruptConfig::default();
        }

        for (i, cpu) in self.cpu_interfaces.iter_mut().enumerate() {
            *cpu = CpuInterface::new(i as u8);
        }
    }

    /// Get CPU interface.
    pub fn cpu(&self, cpu_id: usize) -> Option<&CpuInterface> {
        self.cpu_interfaces.get(cpu_id)
    }

    /// Get mutable CPU interface.
    pub fn cpu_mut(&mut self, cpu_id: usize) -> Option<&mut CpuInterface> {
        self.cpu_interfaces.get_mut(cpu_id)
    }

    /// Set interrupt pending.
    pub fn set_pending(&mut self, intid: u32) {
        if intid >= SPI_START && intid <= SPI_END {
            let idx = (intid - SPI_START) as usize;
            if idx < self.spis.len() {
                self.spis[idx].state.set_pending();
                self.update_all_cpus();
            }
        }
    }

    /// Clear interrupt pending.
    pub fn clear_pending(&mut self, intid: u32) {
        if intid >= SPI_START && intid <= SPI_END {
            let idx = (intid - SPI_START) as usize;
            if idx < self.spis.len() {
                self.spis[idx].state.clear_pending();
                self.update_all_cpus();
            }
        }
    }

    /// Set interrupt enabled.
    pub fn set_enabled(&mut self, intid: u32, enabled: bool) {
        if intid >= SPI_START && intid <= SPI_END {
            let idx = (intid - SPI_START) as usize;
            if idx < self.spis.len() {
                self.spis[idx].enabled = enabled;
                self.update_all_cpus();
            }
        }
    }

    /// Set interrupt priority.
    pub fn set_priority(&mut self, intid: u32, priority: u8) {
        if intid >= SPI_START && intid <= SPI_END {
            let idx = (intid - SPI_START) as usize;
            if idx < self.spis.len() {
                self.spis[idx].priority = priority;
                self.update_all_cpus();
            }
        }
    }

    /// Set interrupt target CPUs.
    pub fn set_targets(&mut self, intid: u32, targets: u8) {
        if intid >= SPI_START && intid <= SPI_END {
            let idx = (intid - SPI_START) as usize;
            if idx < self.spis.len() {
                self.spis[idx].targets = targets;
                self.update_all_cpus();
            }
        }
    }

    /// Set interrupt routing (GICv3).
    pub fn set_routing(&mut self, intid: u32, routing: RoutingMode) {
        if intid >= SPI_START && intid <= SPI_END {
            let idx = (intid - SPI_START) as usize;
            if idx < self.spis.len() {
                self.spis[idx].routing = routing;
                self.update_all_cpus();
            }
        }
    }

    /// Send SGI to specified CPUs.
    pub fn send_sgi(&mut self, intid: u32, target_list: u16, irm: bool) {
        if intid >= NUM_SGI as u32 {
            return;
        }

        if irm {
            // Send to all other PEs
            for cpu in &mut self.cpu_interfaces {
                cpu.send_sgi(intid);
            }
        } else {
            // Send to specified PEs
            for (i, cpu) in self.cpu_interfaces.iter_mut().enumerate() {
                if (target_list >> i) & 1 != 0 {
                    cpu.send_sgi(intid);
                }
            }
        }

        self.update_all_cpus();
    }

    /// Check if CPU has pending interrupt.
    pub fn pending_interrupt(&self, cpu_id: usize) -> bool {
        self.cpu_interfaces
            .get(cpu_id)
            .map(|cpu| cpu.pending_interrupt())
            .unwrap_or(false)
    }

    /// Acknowledge interrupt for CPU.
    pub fn acknowledge(&mut self, cpu_id: usize) -> u32 {
        if let Some(cpu) = self.cpu_interfaces.get_mut(cpu_id) {
            let intid = cpu.acknowledge(&mut self.spis);
            self.update_all_cpus();
            intid
        } else {
            INTID_SPURIOUS
        }
    }

    /// End of interrupt for CPU.
    pub fn end_of_interrupt(&mut self, cpu_id: usize, intid: u32) {
        if let Some(cpu) = self.cpu_interfaces.get_mut(cpu_id) {
            cpu.end_of_interrupt(intid, &mut self.spis);
            self.update_all_cpus();
        }
    }

    /// Update pending state for all CPUs.
    fn update_all_cpus(&mut self) {
        for cpu in &mut self.cpu_interfaces {
            cpu.update_pending(&self.spis);
        }
    }

    // =========================================================================
    // Distributor Register Access
    // =========================================================================

    /// Read distributor register.
    pub fn read_dist(&self, offset: u64) -> u32 {
        match offset {
            // GICD_CTLR
            0x0000 => {
                let mut val = 0u32;
                if self.dist_enabled {
                    val |= 1; // EnableGrp0
                    val |= 2; // EnableGrp1
                }
                if self.are_s {
                    val |= 1 << 4; // ARE_S
                }
                if self.are_ns {
                    val |= 1 << 5; // ARE_NS
                }
                if self.security_enabled {
                    val |= 1 << 6; // DS
                }
                val
            }
            // GICD_TYPER
            0x0004 => {
                let it_lines = (self.config.num_spis / 32).saturating_sub(1) as u32;
                let cpu_number = (self.config.num_cpus - 1) as u32;
                let security = if self.security_enabled { 1 } else { 0 };
                let mbis = 0; // No message-based interrupts
                let lpis = 0; // No LPIs
                let dvis = 0; // No direct virtual LPI injection
                let id_bits = 0b1001; // 10 bits of INTID
                let a3v = 1; // Affinity 3 valid
                let no1n = 0; // 1-of-N supported

                (it_lines & 0x1F)
                    | ((cpu_number & 0x7) << 5)
                    | (security << 10)
                    | (mbis << 16)
                    | (lpis << 17)
                    | (dvis << 18)
                    | ((id_bits & 0x1F) << 19)
                    | (a3v << 24)
                    | (no1n << 25)
            }
            // GICD_IIDR
            0x0008 => {
                // Implementer ID (ARM = 0x43B)
                0x0200_043B
            }
            // GICD_TYPER2
            0x000C => 0,
            // GICD_IGROUPR0-31 (Interrupt Group Registers)
            0x0080..=0x00FC => {
                let reg = ((offset - 0x0080) / 4) as usize;
                let base = reg * 32;
                let mut val = 0u32;
                for bit in 0..32 {
                    let intid = base + bit;
                    if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len()
                    {
                        if self.spis[intid - SPI_START as usize].group == 1 {
                            val |= 1 << bit;
                        }
                    }
                }
                val
            }
            // GICD_ISENABLER0-31 (Interrupt Set-Enable)
            0x0100..=0x017C => {
                let reg = ((offset - 0x0100) / 4) as usize;
                let base = reg * 32;
                let mut val = 0u32;
                for bit in 0..32 {
                    let intid = base + bit;
                    if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len()
                    {
                        if self.spis[intid - SPI_START as usize].enabled {
                            val |= 1 << bit;
                        }
                    }
                }
                val
            }
            // GICD_ISPENDR0-31 (Interrupt Set-Pending)
            0x0200..=0x027C => {
                let reg = ((offset - 0x0200) / 4) as usize;
                let base = reg * 32;
                let mut val = 0u32;
                for bit in 0..32 {
                    let intid = base + bit;
                    if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len()
                    {
                        if self.spis[intid - SPI_START as usize].state.is_pending() {
                            val |= 1 << bit;
                        }
                    }
                }
                val
            }
            // GICD_IPRIORITYR0-254 (Interrupt Priority)
            0x0400..=0x07F8 => {
                let reg = ((offset - 0x0400) / 4) as usize;
                let base = reg * 4;
                let mut val = 0u32;
                for byte in 0..4 {
                    let intid = base + byte;
                    if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len()
                    {
                        val |=
                            (self.spis[intid - SPI_START as usize].priority as u32) << (byte * 8);
                    }
                }
                val
            }
            // GICD_ITARGETSR0-254 (Interrupt Processor Targets) - GICv2 only
            0x0800..=0x08FC => 0x0101_0101, // All to CPU0
            // GICD_ICFGR0-63 (Interrupt Configuration)
            0x0C00..=0x0CFC => {
                let reg = ((offset - 0x0C00) / 4) as usize;
                let base = reg * 16;
                let mut val = 0u32;
                for nibble in 0..16 {
                    let intid = base + nibble;
                    if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len()
                    {
                        if self.spis[intid - SPI_START as usize].trigger == TriggerType::Edge {
                            val |= 2 << (nibble * 2);
                        }
                    }
                }
                val
            }
            // GICD_PIDR2
            0xFFE8 => {
                // GICv3
                0x3B
            }
            _ => 0,
        }
    }

    /// Write distributor register.
    pub fn write_dist(&mut self, offset: u64, value: u32) {
        match offset {
            // GICD_CTLR
            0x0000 => {
                self.dist_enabled = (value & 0x3) != 0;
                self.are_s = (value >> 4) & 1 != 0;
                self.are_ns = (value >> 5) & 1 != 0;
            }
            // GICD_IGROUPR0-31
            0x0080..=0x00FC => {
                let reg = ((offset - 0x0080) / 4) as usize;
                let base = reg * 32;
                for bit in 0..32 {
                    let intid = base + bit;
                    if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len()
                    {
                        self.spis[intid - SPI_START as usize].group =
                            if (value >> bit) & 1 != 0 { 1 } else { 0 };
                    }
                }
            }
            // GICD_ISENABLER0-31
            0x0100..=0x017C => {
                let reg = ((offset - 0x0100) / 4) as usize;
                let base = reg * 32;
                for bit in 0..32 {
                    if (value >> bit) & 1 != 0 {
                        let intid = base + bit;
                        if intid >= SPI_START as usize
                            && (intid - SPI_START as usize) < self.spis.len()
                        {
                            self.spis[intid - SPI_START as usize].enabled = true;
                        }
                    }
                }
                self.update_all_cpus();
            }
            // GICD_ICENABLER0-31
            0x0180..=0x01FC => {
                let reg = ((offset - 0x0180) / 4) as usize;
                let base = reg * 32;
                for bit in 0..32 {
                    if (value >> bit) & 1 != 0 {
                        let intid = base + bit;
                        if intid >= SPI_START as usize
                            && (intid - SPI_START as usize) < self.spis.len()
                        {
                            self.spis[intid - SPI_START as usize].enabled = false;
                        }
                    }
                }
                self.update_all_cpus();
            }
            // GICD_ISPENDR0-31
            0x0200..=0x027C => {
                let reg = ((offset - 0x0200) / 4) as usize;
                let base = reg * 32;
                for bit in 0..32 {
                    if (value >> bit) & 1 != 0 {
                        let intid = base + bit;
                        if intid >= SPI_START as usize
                            && (intid - SPI_START as usize) < self.spis.len()
                        {
                            self.spis[intid - SPI_START as usize].state.set_pending();
                        }
                    }
                }
                self.update_all_cpus();
            }
            // GICD_ICPENDR0-31
            0x0280..=0x02FC => {
                let reg = ((offset - 0x0280) / 4) as usize;
                let base = reg * 32;
                for bit in 0..32 {
                    if (value >> bit) & 1 != 0 {
                        let intid = base + bit;
                        if intid >= SPI_START as usize
                            && (intid - SPI_START as usize) < self.spis.len()
                        {
                            self.spis[intid - SPI_START as usize].state.clear_pending();
                        }
                    }
                }
                self.update_all_cpus();
            }
            // GICD_IPRIORITYR0-254
            0x0400..=0x07F8 => {
                let reg = ((offset - 0x0400) / 4) as usize;
                let base = reg * 4;
                for byte in 0..4 {
                    let intid = base + byte;
                    if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len()
                    {
                        self.spis[intid - SPI_START as usize].priority =
                            ((value >> (byte * 8)) & 0xFF) as u8;
                    }
                }
                self.update_all_cpus();
            }
            // GICD_ICFGR0-63
            0x0C00..=0x0CFC => {
                let reg = ((offset - 0x0C00) / 4) as usize;
                let base = reg * 16;
                for nibble in 0..16 {
                    let intid = base + nibble;
                    if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len()
                    {
                        self.spis[intid - SPI_START as usize].trigger =
                            if (value >> (nibble * 2 + 1)) & 1 != 0 {
                                TriggerType::Edge
                            } else {
                                TriggerType::Level
                            };
                    }
                }
            }
            // GICD_IROUTER32-1019 (Interrupt Routing)
            0x6100..=0x7FD8 => {
                let intid = ((offset - 0x6100) / 8) as usize + SPI_START as usize;
                if intid >= SPI_START as usize && (intid - SPI_START as usize) < self.spis.len() {
                    let irm = (value >> 31) & 1 != 0;
                    let affinity = (value & 0x00FF_FFFF) as u64;
                    self.spis[intid - SPI_START as usize].routing = if irm {
                        RoutingMode::Any
                    } else {
                        RoutingMode::Affinity(affinity)
                    };
                }
                self.update_all_cpus();
            }
            _ => {}
        }
    }
}

impl Default for Gic {
    fn default() -> Self {
        Self::new(GicConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interrupt_state() {
        let mut state = InterruptState::Inactive;
        assert!(!state.is_pending());
        assert!(!state.is_active());

        state.set_pending();
        assert!(state.is_pending());
        assert!(!state.is_active());

        state.set_active();
        assert!(!state.is_pending());
        assert!(state.is_active());

        state.set_pending();
        assert!(state.is_pending());
        assert!(state.is_active());
        assert_eq!(state, InterruptState::ActivePending);
    }

    #[test]
    fn test_gic_creation() {
        let gic = Gic::new(GicConfig::default());
        assert!(!gic.pending_interrupt(0));
    }

    #[test]
    fn test_sgi() {
        let mut gic = Gic::new(GicConfig::default());

        // Enable group 1 and set priority mask
        if let Some(cpu) = gic.cpu_mut(0) {
            cpu.enable_grp1 = true;
            cpu.priority_mask = 0xFF;
            cpu.private_interrupts[0].enabled = true;
            cpu.private_interrupts[0].priority = 0x80;
        }

        // Send SGI
        gic.send_sgi(0, 1, false);

        // Should have pending interrupt
        assert!(gic.pending_interrupt(0));

        // Acknowledge
        let intid = gic.acknowledge(0);
        assert_eq!(intid, 0);

        // End of interrupt
        gic.end_of_interrupt(0, intid);
        assert!(!gic.pending_interrupt(0));
    }

    #[test]
    fn test_spi() {
        let mut gic = Gic::new(GicConfig::default());

        // Configure SPI 32
        gic.set_enabled(32, true);
        gic.set_priority(32, 0x80);
        gic.set_routing(32, RoutingMode::Affinity(0));

        // Enable CPU interface
        if let Some(cpu) = gic.cpu_mut(0) {
            cpu.enable_grp1 = true;
            cpu.priority_mask = 0xFF;
        }

        // Set pending
        gic.set_pending(32);
        assert!(gic.pending_interrupt(0));

        // Acknowledge and EOI
        let intid = gic.acknowledge(0);
        assert_eq!(intid, 32);

        gic.end_of_interrupt(0, intid);
        // Level-sensitive - still pending
        assert!(gic.pending_interrupt(0));

        // Clear pending
        gic.clear_pending(32);
        assert!(!gic.pending_interrupt(0));
    }

    #[test]
    fn test_dist_registers() {
        let gic = Gic::new(GicConfig::default());

        // GICD_TYPER
        let typer = gic.read_dist(0x0004);
        assert!((typer & 0x1F) > 0); // IT_Lines

        // GICD_IIDR
        let iidr = gic.read_dist(0x0008);
        assert_eq!(iidr & 0xFFF, 0x43B); // ARM implementer
    }
}
