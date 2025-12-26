use std::path::PathBuf;

use clap::Parser;

use rax::config::{
    Address, ArchKind, BackendKind, CliConfig, Endianness, FileConfig, HexagonIsa, MemorySize,
    VmConfig,
};
use rax::Result;
use rax::vmm::Vmm;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "rax", about = "Minimal KVM-based hypervisor for x86_64")]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long, value_enum)]
    arch: Option<ArchKind>,
    #[arg(long, value_enum)]
    backend: Option<BackendKind>,
    #[arg(long, value_parser = clap::value_parser!(MemorySize))]
    memory: Option<MemorySize>,
    #[arg(long)]
    vcpus: Option<u8>,
    #[arg(long)]
    kernel: Option<PathBuf>,
    #[arg(long)]
    initrd: Option<PathBuf>,
    #[arg(long)]
    cmdline: Option<String>,
    #[arg(long, value_enum)]
    hexagon_isa: Option<HexagonIsa>,
    #[arg(long, value_enum)]
    hexagon_endian: Option<Endianness>,
    #[arg(long, value_parser = clap::value_parser!(Address))]
    hexagon_entry: Option<Address>,
    #[arg(long, value_parser = clap::value_parser!(Address))]
    hexagon_load_addr: Option<Address>,
    /// Output instruction trace file (SDE-compatible format)
    #[arg(long)]
    trace: Option<PathBuf>,
    /// Enable GDB server on specified port (requires --features debug)
    #[arg(long)]
    gdb: Option<u16>,
    /// Wait for GDB connection before starting execution
    #[arg(long)]
    wait_gdb: bool,
    /// Enable GDB protocol tracing (shows all RX/TX packets)
    #[arg(long)]
    gdb_trace: bool,
}

fn main() -> Result<()> {
    // Parse CLI first so we can check --gdb-trace
    let cli = Cli::parse();

    // Build log filter, adding gdb trace if requested
    let base_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let filter = if cli.gdb_trace {
        base_filter.add_directive("rax::gdb=trace".parse().unwrap())
    } else {
        base_filter
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
    let file_config = match cli.config.as_deref() {
        Some(path) => Some(FileConfig::load(path)?),
        None => None,
    };
    let cli_config = CliConfig {
        arch: cli.arch,
        backend: cli.backend,
        memory: cli.memory,
        vcpus: cli.vcpus,
        kernel: cli.kernel,
        initrd: cli.initrd,
        cmdline: cli.cmdline,
        hexagon_isa: cli.hexagon_isa,
        hexagon_endian: cli.hexagon_endian,
        hexagon_entry: cli.hexagon_entry,
        hexagon_load_addr: cli.hexagon_load_addr,
        trace: cli.trace,
        gdb_port: cli.gdb,
        wait_gdb: cli.wait_gdb,
    };

    let config = VmConfig::from_sources(cli_config, file_config)?;
    let mut vmm = Vmm::new(config)?;
    vmm.run()?;

    Ok(())
}
