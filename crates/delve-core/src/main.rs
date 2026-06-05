use std::path::PathBuf;

use clap::{Parser, Subcommand};

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

    match &cli.command {
        Some(Commands::Audit) => {
            println!("Delve Audit — {}\n", root.display());
            let output = delve_core::unused::run_deadcode(root, cli.json);
            println!("{}", output);
        }
        Some(Commands::Deadcode) => {
            let output = delve_core::unused::run_deadcode(root, cli.json);
            print!("{}", output);
        }
        None => {
            // Default command: audit
            println!("Delve Audit — {}\n", root.display());
            let output = delve_core::unused::run_deadcode(root, cli.json);
            println!("{}", output);
        }
        Some(Commands::Split) => {
            let output = delve_core::giant_funcs::run_split(root, cli.json);
            print!("{}", output);
        }
        Some(Commands::Dup) => {
            println!("Dup command not yet implemented");
        }
        Some(Commands::Health) => {
            println!("Health command not yet implemented");
        }
    }
}
