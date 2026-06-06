use std::path::PathBuf;

use clap::{Parser, Subcommand};

use delve_core::config::DelveConfig;

#[derive(Parser)]
#[command(name = "glimpse", version, about = "Static analysis for TS/JS")]
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

    /// Fail if health score is below this threshold (audit/health only)
    #[arg(long, default_value_t = 40, global = true)]
    fail_on: usize,

    /// Output GitHub Actions annotations
    #[arg(long, global = true)]
    github_annotations: bool,

    /// Output SARIF format
    #[arg(long, global = true)]
    sarif: bool,
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
    /// Dependency audit: unused and missing packages
    Deps,
    /// Single 0-100 health score and todo list
    Health,
}

fn main() {
    let cli = Cli::parse();
    let root = &cli.path;
    let config = DelveConfig::load(cli.config.as_deref(), root);
    let json = cli.json || cli.sarif;
    let annotations = cli.github_annotations;

    let result = match &cli.command {
        Some(Commands::Audit) => {
            delve_core::report::run_full_audit(root, json, cli.sarif, annotations, &config)
        }
        Some(Commands::Deadcode) => {
            delve_core::unused::run_deadcode(root, json, annotations, &config)
        }
        Some(Commands::Split) => {
            delve_core::giant_funcs::run_split(root, json, &config)
        }
        Some(Commands::Dup) => {
            delve_core::duplicates::run_dup(root, json, &config)
        }
        Some(Commands::Deps) => {
            delve_core::unused_deps::run_deps(root, json, &config)
        }
        Some(Commands::Health) => {
            delve_core::health::run_health(root, json, &config)
        }
        None => {
            delve_core::report::run_full_audit(root, json, cli.sarif, annotations, &config)
        }
    };

    print!("{}", result.output);
    eprintln!("DEBUG: score={}, fail_on={}, exit_code={}", result.score, cli.fail_on, result.exit_code);

    if result.score < cli.fail_on {
        eprintln!("DEBUG: failing due to --fail-on threshold");
        std::process::exit(1);
    }

    if cli.sarif {
        // SARIF implies exit code 0 for valid output
        return;
    }

    std::process::exit(result.exit_code);
}
