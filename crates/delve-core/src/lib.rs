pub mod parser;
pub mod graph;
pub mod unused;
pub mod unused_deps;
pub mod giant_funcs;
pub mod duplicates;
pub mod risks;
pub mod health;
pub mod report;
pub mod config;
pub mod progress;

/// Result from a command execution, containing the output string and exit code.
pub struct CommandResult {
    pub output: String,
    pub exit_code: i32,
    pub score: usize,
}
