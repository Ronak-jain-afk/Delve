use std::path::PathBuf;

use clap::{Parser, Subcommand};

use delve_core::config::DelveConfig;

#[derive(Parser)]
#[command(name = "delve-core", version, about = "Static analysis for TS/JS")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the project to analyze
    #[arg(short, long, default_value = ".")]
    path: PathBuf,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Config file path
    #[arg(short, long)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Full report: unused code, giant functions, duplicates, risky patterns, health score
    Audit,
    /// Unused exports only
    Deadcode,
    /// Giant functions with line ranges and complexity
    Split,
    /// Duplicate code blocks with locations
    Dup,
    /// Single 0-100 health score and todo list
    Health,
}

fn main() {
    let cli = Cli::parse();
    let root = &cli.path;
    let config = DelveConfig::load(cli.config.as_deref(), root);

    match &cli.command {
        Some(Commands::Audit) => {
            let output = delve_core::report::run_full_audit(root, cli.json, &config);
            print!("{}", output);
        }
        Some(Commands::Deadcode) => {
            let output = delve_core::unused::run_deadcode(root, cli.json, &config);
            print!("{}", output);
        }
        Some(Commands::Split) => {
            let output = delve_core::giant_funcs::run_split(root, cli.json, &config);
            print!("{}", output);
        }
        Some(Commands::Dup) => {
            let output = delve_core::duplicates::run_dup(root, cli.json, &config);
            print!("{}", output);
        }
        Some(Commands::Health) => {
            let output = delve_core::health::run_health(root, cli.json, &config);
            print!("{}", output);
        }
        None => {
            let output = delve_core::report::run_full_audit(root, cli.json, &config);
            print!("{}", output);
        }
    }
}
