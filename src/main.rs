use std::path::PathBuf;

use clap::Parser;

use rax::config::{ArchKind, BackendKind, CliConfig, FileConfig, MemorySize, VmConfig};
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
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    let cli = Cli::parse();
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
    };

    let config = VmConfig::from_sources(cli_config, file_config)?;
    let mut vmm = Vmm::new(config)?;
    vmm.run()?;

    Ok(())
}
