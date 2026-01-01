//! ASL Parser CLI
//!
//! A command-line tool for parsing ARM Architecture Specification Language files.

use asl_parser::{parse_definitions, parse_instructions, parse_registers};
use clap::{Parser, Subcommand, ValueEnum};
use miette::{IntoDiagnostic, Result, WrapErr};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "asl-parser",
    about = "Parser for ARM Architecture Specification Language (ASL)",
    version,
    author
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse definition files (types, functions, constants, etc.)
    #[command(alias = "defs")]
    Definitions {
        /// Input ASL file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Output format
        #[arg(short, long, default_value = "summary")]
        format: OutputFormat,
    },

    /// Parse instruction definitions
    #[command(alias = "inst")]
    Instructions {
        /// Input ASL file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Output format
        #[arg(short, long, default_value = "summary")]
        format: OutputFormat,
    },

    /// Parse register definitions
    #[command(alias = "regs")]
    Registers {
        /// Input ASL file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Output format
        #[arg(short, long, default_value = "summary")]
        format: OutputFormat,
    },

    /// Lex a file and print tokens (for debugging)
    Lex {
        /// Input ASL file
        file: PathBuf,
    },

    /// Download ARM XML specification files
    #[cfg(feature = "xml-extract")]
    Download {
        /// Output directory for downloaded files
        #[arg(short, long, default_value = "xml")]
        output: PathBuf,

        /// ARM specification version
        #[arg(short, long, default_value = asl_parser::xml::DEFAULT_VERSION)]
        version: String,

        /// Specification type(s) to download
        #[arg(short, long, value_enum)]
        spec: Option<Vec<SpecTypeArg>>,
    },

    /// Extract ASL from downloaded XML files
    #[cfg(feature = "xml-extract")]
    Extract {
        /// Input XML directory (containing downloaded specs)
        #[arg(short, long, default_value = "xml")]
        input: PathBuf,

        /// Output directory for ASL files
        #[arg(short, long, default_value = "asl")]
        output: PathBuf,

        /// ARM specification version
        #[arg(long, default_value = asl_parser::xml::DEFAULT_VERSION)]
        version: String,

        /// Demangle instruction ASL code
        #[arg(long)]
        demangle: bool,

        /// Filter to specific architecture(s)
        #[arg(long)]
        arch: Option<Vec<String>>,
    },

    /// Full pipeline: download, extract, and parse
    #[cfg(feature = "xml-extract")]
    Pipeline {
        /// Working directory for all files
        #[arg(short, long, default_value = ".")]
        workdir: PathBuf,

        /// ARM specification version
        #[arg(long, default_value = asl_parser::xml::DEFAULT_VERSION)]
        version: String,

        /// Demangle instruction ASL code
        #[arg(long)]
        demangle: bool,

        /// Output format for parsed results
        #[arg(short, long, default_value = "summary")]
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, ValueEnum, Default)]
enum OutputFormat {
    /// Print a summary of parsed items
    #[default]
    Summary,
    /// Print detailed debug output
    Debug,
    /// Print as JSON (requires 'serde' feature)
    #[cfg(feature = "serde")]
    Json,
}

#[cfg(feature = "xml-extract")]
#[derive(Clone, Copy, ValueEnum)]
enum SpecTypeArg {
    /// A64 instruction set
    A64,
    /// AArch32 instruction set
    AArch32,
    /// System registers
    SysReg,
    /// All specifications
    All,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Definitions { files, format } => {
            for file in files {
                parse_defs_file(&file, format)?;
            }
        }
        Command::Instructions { files, format } => {
            for file in files {
                parse_instrs_file(&file, format)?;
            }
        }
        Command::Registers { files, format } => {
            for file in files {
                parse_regs_file(&file, format)?;
            }
        }
        Command::Lex { file } => {
            lex_file(&file)?;
        }
        #[cfg(feature = "xml-extract")]
        Command::Download {
            output,
            version,
            spec,
        } => {
            download_specs(&output, &version, spec)?;
        }
        #[cfg(feature = "xml-extract")]
        Command::Extract {
            input,
            output,
            version,
            demangle,
            arch,
        } => {
            extract_asl(&input, &output, &version, demangle, arch)?;
        }
        #[cfg(feature = "xml-extract")]
        Command::Pipeline {
            workdir,
            version,
            demangle,
            format,
        } => {
            run_pipeline(&workdir, &version, demangle, format)?;
        }
    }

    Ok(())
}

fn parse_defs_file(path: &PathBuf, format: OutputFormat) -> Result<()> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read file: {}", path.display()))?;

    let defs = parse_definitions(&source).map_err(|e| {
        miette::miette!(
            labels = vec![miette::LabeledSpan::at(
                e.span.start..e.span.end,
                e.kind.to_string()
            )],
            "parse error in {}",
            path.display()
        )
        .with_source_code(source.clone())
    })?;

    match format {
        OutputFormat::Summary => {
            println!("Parsed {} definitions from {}", defs.len(), path.display());
            for def in &defs {
                print_def_summary(def);
            }
        }
        OutputFormat::Debug => {
            println!("=== {} ===", path.display());
            for def in &defs {
                println!("{:#?}", def);
            }
        }
        #[cfg(feature = "serde")]
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&defs).into_diagnostic()?;
            println!("{}", json);
        }
    }

    Ok(())
}

fn parse_instrs_file(path: &PathBuf, format: OutputFormat) -> Result<()> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read file: {}", path.display()))?;

    let instrs = parse_instructions(&source).map_err(|e| {
        miette::miette!(
            labels = vec![miette::LabeledSpan::at(
                e.span.start..e.span.end,
                e.kind.to_string()
            )],
            "parse error in {}",
            path.display()
        )
        .with_source_code(source.clone())
    })?;

    match format {
        OutputFormat::Summary => {
            println!(
                "Parsed {} instructions from {}",
                instrs.len(),
                path.display()
            );
            for instr in &instrs {
                println!(
                    "  {} ({} encoding{})",
                    instr.name,
                    instr.encodings.len(),
                    if instr.encodings.len() == 1 { "" } else { "s" }
                );
            }
        }
        OutputFormat::Debug => {
            println!("=== {} ===", path.display());
            for instr in &instrs {
                println!("{:#?}", instr);
            }
        }
        #[cfg(feature = "serde")]
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&instrs).into_diagnostic()?;
            println!("{}", json);
        }
    }

    Ok(())
}

fn parse_regs_file(path: &PathBuf, format: OutputFormat) -> Result<()> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read file: {}", path.display()))?;

    let regs = parse_registers(&source).map_err(|e| {
        miette::miette!(
            labels = vec![miette::LabeledSpan::at(
                e.span.start..e.span.end,
                e.kind.to_string()
            )],
            "parse error in {}",
            path.display()
        )
        .with_source_code(source.clone())
    })?;

    match format {
        OutputFormat::Summary => {
            println!(
                "Parsed {} register definitions from {}",
                regs.len(),
                path.display()
            );
            for reg in &regs {
                match reg {
                    asl_parser::RegisterDefinition::Single(r) => {
                        println!("  {} ({}b, {} fields)", r.name, r.width, r.fields.len());
                    }
                    asl_parser::RegisterDefinition::Array(arr) => {
                        println!(
                            "  {}[{}..{}] ({}b, {} fields)",
                            arr.register.name,
                            arr.index_min,
                            arr.index_max,
                            arr.register.width,
                            arr.register.fields.len()
                        );
                    }
                }
            }
        }
        OutputFormat::Debug => {
            println!("=== {} ===", path.display());
            for reg in &regs {
                println!("{:#?}", reg);
            }
        }
        #[cfg(feature = "serde")]
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&regs).into_diagnostic()?;
            println!("{}", json);
        }
    }

    Ok(())
}

fn lex_file(path: &PathBuf) -> Result<()> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read file: {}", path.display()))?;

    let mut lexer = asl_parser::lexer::Lexer::new(&source);
    let tokens = lexer.collect_tokens();

    println!("=== Tokens from {} ===", path.display());
    for tok in tokens {
        let text = &source[tok.span.start..tok.span.end.min(source.len())];
        println!(
            "{:4}..{:<4} {:20} {:?}",
            tok.span.start,
            tok.span.end,
            format!("{:?}", tok.kind),
            if text.len() > 30 {
                format!("{}...", &text[..30])
            } else {
                text.to_string()
            }
        );
    }

    Ok(())
}

#[cfg(feature = "xml-extract")]
fn download_specs(output: &PathBuf, version: &str, spec: Option<Vec<SpecTypeArg>>) -> Result<()> {
    use asl_parser::xml::download::{download_spec, extract_archive};
    use asl_parser::xml::SpecType;

    let specs = spec.unwrap_or_else(|| vec![SpecTypeArg::All]);

    let download_a64 = specs
        .iter()
        .any(|s| matches!(s, SpecTypeArg::A64 | SpecTypeArg::All));
    let download_a32 = specs
        .iter()
        .any(|s| matches!(s, SpecTypeArg::AArch32 | SpecTypeArg::All));
    let download_sysreg = specs
        .iter()
        .any(|s| matches!(s, SpecTypeArg::SysReg | SpecTypeArg::All));

    if download_a64 || download_a32 || download_sysreg {
        fs::create_dir_all(output)
            .into_diagnostic()
            .wrap_err("Failed to create output directory")?;
    }

    if download_a64 {
        let archive =
            download_spec(SpecType::A64, version, output).map_err(|e| miette::miette!("{}", e))?;
        extract_archive(&archive, output).map_err(|e| miette::miette!("{}", e))?;
    }

    if download_a32 {
        let archive = download_spec(SpecType::AArch32, version, output)
            .map_err(|e| miette::miette!("{}", e))?;
        extract_archive(&archive, output).map_err(|e| miette::miette!("{}", e))?;
    }

    if download_sysreg {
        let archive = download_spec(SpecType::SysReg, version, output)
            .map_err(|e| miette::miette!("{}", e))?;
        extract_archive(&archive, output).map_err(|e| miette::miette!("{}", e))?;
    }

    println!("Download complete!");
    Ok(())
}

#[cfg(feature = "xml-extract")]
fn extract_asl(
    input: &PathBuf,
    output: &PathBuf,
    version: &str,
    demangle: bool,
    arch: Option<Vec<String>>,
) -> Result<()> {
    use asl_parser::xml::{extract_from_dirs, regs, shared, ExtractConfig, SpecType};

    // Find XML directories
    let a64_dir = input.join(SpecType::A64.extracted_dir(version));
    let a32_dir = input.join(SpecType::AArch32.extracted_dir(version));
    let sysreg_dir = input.join(SpecType::SysReg.extracted_dir(version));

    let mut dirs: Vec<&std::path::Path> = Vec::new();
    if a64_dir.exists() {
        dirs.push(a64_dir.as_path());
    }
    if a32_dir.exists() {
        dirs.push(a32_dir.as_path());
    }

    if dirs.is_empty() {
        return Err(miette::miette!(
            "No XML directories found in {}. Run 'download' first.",
            input.display()
        ));
    }

    let config = ExtractConfig {
        demangle,
        architectures: arch.unwrap_or_default(),
        ..Default::default()
    };

    println!("Extracting ASL from XML files...");
    let result = extract_from_dirs(&dirs, &config).map_err(|e| miette::miette!("{}", e))?;

    // Create output directory
    fs::create_dir_all(output)
        .into_diagnostic()
        .wrap_err("Failed to create output directory")?;

    // Write definitions
    let defs_path = output.join("arm_defs.asl");
    let defs_content = result.generate_defs_asl();
    fs::write(&defs_path, &defs_content)
        .into_diagnostic()
        .wrap_err("Failed to write arm_defs.asl")?;
    println!(
        "Wrote {} ({} bytes)",
        defs_path.display(),
        defs_content.len()
    );

    // Write instructions
    let instrs_path = output.join("arm_instrs.asl");
    let instrs_content = result.generate_instrs_asl();
    fs::write(&instrs_path, &instrs_content)
        .into_diagnostic()
        .wrap_err("Failed to write arm_instrs.asl")?;
    println!(
        "Wrote {} ({} bytes)",
        instrs_path.display(),
        instrs_content.len()
    );

    // Extract and write registers if sysreg directory exists
    if sysreg_dir.exists() {
        println!("Extracting register definitions...");
        let registers = regs::read_registers(&sysreg_dir).map_err(|e| miette::miette!("{}", e))?;

        // Read notice for registers
        let notice = if let Ok(n) = shared::read_notice(&a64_dir.join("notice.xml")) {
            n
        } else {
            result.notice.clone()
        };

        let regs_content = regs::generate_registers_asl(&registers, &notice);
        let regs_path = output.join("arm_regs.asl");
        fs::write(&regs_path, &regs_content)
            .into_diagnostic()
            .wrap_err("Failed to write arm_regs.asl")?;
        println!(
            "Wrote {} ({} bytes)",
            regs_path.display(),
            regs_content.len()
        );
    }

    println!("Extraction complete!");
    Ok(())
}

#[cfg(feature = "xml-extract")]
fn run_pipeline(
    workdir: &PathBuf,
    version: &str,
    demangle: bool,
    format: OutputFormat,
) -> Result<()> {
    let xml_dir = workdir.join("xml");
    let asl_dir = workdir.join("asl");

    // Step 1: Download
    println!("=== Step 1: Downloading XML specifications ===");
    download_specs(&xml_dir, version, Some(vec![SpecTypeArg::All]))?;

    // Step 2: Extract
    println!("\n=== Step 2: Extracting ASL from XML ===");
    extract_asl(&xml_dir, &asl_dir, version, demangle, None)?;

    // Step 3: Parse
    println!("\n=== Step 3: Parsing extracted ASL ===");

    let defs_path = asl_dir.join("arm_defs.asl");
    if defs_path.exists() {
        println!("\n--- arm_defs.asl ---");
        parse_defs_file(&defs_path, format)?;
    }

    let regs_path = asl_dir.join("arm_regs.asl");
    if regs_path.exists() {
        println!("\n--- arm_regs.asl ---");
        parse_regs_file(&regs_path, format)?;
    }

    let instrs_path = asl_dir.join("arm_instrs.asl");
    if instrs_path.exists() {
        println!("\n--- arm_instrs.asl ---");
        parse_instrs_file(&instrs_path, format)?;
    }

    println!("\n=== Pipeline complete! ===");
    Ok(())
}

fn print_def_summary(def: &asl_parser::Definition) {
    use asl_parser::Definition;
    match def {
        Definition::TypeBuiltin(name) => {
            println!("  builtin type {}", name);
        }
        Definition::TypeAbstract(name) => {
            println!("  type {}", name);
        }
        Definition::TypeAlias { name, .. } => {
            println!("  type {} = ...", name);
        }
        Definition::TypeStruct { name, fields } => {
            println!("  type {} is ({} fields)", name, fields.len());
        }
        Definition::TypeEnum { name, variants } => {
            println!("  enumeration {} {{ {} variants }}", name, variants.len());
        }
        Definition::Variable { name, .. } => {
            println!("  var {}", name);
        }
        Definition::Const { name, .. } => {
            println!("  constant {}", name);
        }
        Definition::Array { name, .. } => {
            println!("  array {}", name);
        }
        Definition::Callable {
            name,
            params,
            returns,
            ..
        } => {
            let kind = if returns.is_empty() {
                "procedure"
            } else {
                "function"
            };
            println!("  {} {}({} params)", kind, name, params.len());
        }
        Definition::Getter { name, params, .. } => {
            let idx = params.as_ref().map(|p| p.len()).unwrap_or(0);
            if idx > 0 {
                println!("  getter {}[{}]", name, idx);
            } else {
                println!("  getter {}", name);
            }
        }
        Definition::Setter { name, params, .. } => {
            let idx = params.as_ref().map(|p| p.len()).unwrap_or(0);
            if idx > 0 {
                println!("  setter {}[{}]", name, idx);
            } else {
                println!("  setter {}", name);
            }
        }
        Definition::Instruction(instr) => {
            println!(
                "  instruction {} ({} encodings)",
                instr.name,
                instr.encodings.len()
            );
        }
        Definition::Decode { iset, cases } => {
            println!("  decode {} ({} cases)", iset, cases.len());
        }
    }
}
