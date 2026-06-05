use std::path::Path;

use crate::config::Weights;
use crate::duplicates;
use crate::giant_funcs::{self, Severity};
use crate::graph::DepGraph;
use crate::risks::{self, RiskKind};
use crate::unused;

pub struct HealthReport {
    pub score: usize,
    pub label: &'static str,
    pub unused_count: usize,
    pub giant_critical: usize,
    pub giant_warning: usize,
    pub duplicate_count: usize,
    pub any_type_count: usize,
    pub console_log_count: usize,
    pub debugger_count: usize,
    pub deep_nesting_count: usize,
    pub long_params_count: usize,
}

impl HealthReport {
    pub fn to_todo_list(&self) -> Vec<String> {
        let mut todos = Vec::new();

        if self.unused_count > 0 {
            todos.push(format!(
                "Remove {} unused export(s) — use `delve deadcode` to list them",
                self.unused_count
            ));
        }
        if self.giant_critical + self.giant_warning > 0 {
            todos.push(format!(
                "Split {} giant function(s) — use `delve split` to see details",
                self.giant_critical + self.giant_warning
            ));
        }
        if self.duplicate_count > 0 {
            todos.push(format!(
                "Refactor {} duplicate block(s) — use `delve dup` to see locations",
                self.duplicate_count
            ));
        }
        if self.any_type_count > 0 {
            todos.push(format!(
                "Replace {} `any` type(s) with proper types",
                self.any_type_count
            ));
        }
        if self.console_log_count > 0 {
            todos.push(format!(
                "Remove {} console.log statement(s) from production code",
                self.console_log_count
            ));
        }
        if self.debugger_count > 0 {
            todos.push(format!(
                "Remove {} debugger statement(s)",
                self.debugger_count
            ));
        }
        if self.deep_nesting_count > 0 {
            todos.push(format!(
                "Reduce nesting depth in {} location(s)",
                self.deep_nesting_count
            ));
        }
        if self.long_params_count > 0 {
            todos.push(format!(
                "Reduce parameter count in {} function(s)",
                self.long_params_count
            ));
        }

        if todos.is_empty() {
            todos.push("Nothing to fix! Your codebase looks healthy.".to_string());
        }

        todos
    }
}

pub fn calculate(graph: &DepGraph, giant_metrics: &[giant_funcs::FunctionMetrics], risk_items: &[risks::RiskItem], weights: &Weights, root: &Path) -> HealthReport {
    let unused_items = unused::find_unused(graph);
    let unused_file_count = unused_items.len();

    let giant_critical = giant_metrics.iter().filter(|m| m.severity == Severity::Critical).count();
    let giant_warning = giant_metrics.iter().filter(|m| m.severity == Severity::Warning).count();

    let any_type_count = risk_items.iter().filter(|r| r.kind == RiskKind::AnyType).count();
    let console_log_count = risk_items.iter().filter(|r| r.kind == RiskKind::ConsoleLog).count();
    let debugger_count = risk_items.iter().filter(|r| r.kind == RiskKind::Debugger).count();
    let deep_nesting_count = risk_items.iter().filter(|r| r.kind == RiskKind::DeepNesting).count();
    let long_params_count = risk_items.iter().filter(|r| r.kind == RiskKind::LongParams).count();

    // Count duplicates
    let files = crate::parser::find_source_files(root);
    let dup_clusters = duplicates::find_duplicates(&files);
    let duplicate_count = dup_clusters.len();

    // Calculate score
    let mut score: isize = 100;
    if unused_file_count > 0 {
        score -= weights.unused_file as isize;
    }
    score -= (giant_critical * weights.giant_critical) as isize;
    score -= (giant_warning * weights.giant_warning) as isize;
    score -= (duplicate_count * weights.duplicate) as isize;
    score -= (any_type_count * weights.any_type) as isize;
    score -= (console_log_count * weights.console_log) as isize;

    let score = score.max(0) as usize;

    let label = if score >= 70 {
        "healthy"
    } else if score >= 40 {
        "needs work"
    } else {
        "vibe disaster"
    };

    HealthReport {
        score,
        label,
        unused_count: unused_file_count,
        giant_critical,
        giant_warning,
        duplicate_count,
        any_type_count,
        console_log_count,
        debugger_count,
        deep_nesting_count,
        long_params_count,
    }
}

pub fn run_health(root: &Path, json: bool, config: &crate::config::DelveConfig) -> String {
    let progress = crate::progress::Progress::new(!json);
    progress.set_message("Parsing files...");
    let symbols = crate::parser::parse_all_files(root);
    progress.set_message("Analyzing dependencies...");
    let mut graph = crate::graph::DepGraph::new(symbols);
    graph.build();
    graph.detect_entry_points();
    graph.traverse_from_entry_points();

    progress.set_message("Analyzing giant functions...");
    let all_symbols: Vec<_> = graph.all_symbols.values().cloned().collect();
    let giant_metrics = giant_funcs::analyze_functions(&all_symbols, &config.thresholds);
    progress.set_message("Detecting risky patterns...");
    let risk_items = risks::detect_risks(root);

    progress.set_message("Calculating health score...");
    let report = calculate(&graph, &giant_metrics, &risk_items, &config.weights, root);
    progress.finish();

    if json {
        serde_json::to_string_pretty(&serde_json::json!({
            "score": report.score,
            "label": report.label,
            "unusedCount": report.unused_count,
            "giantCritical": report.giant_critical,
            "giantWarning": report.giant_warning,
            "duplicateCount": report.duplicate_count,
            "anyTypes": report.any_type_count,
            "consoleLogs": report.console_log_count,
            "debuggerStatements": report.debugger_count,
            "deepNesting": report.deep_nesting_count,
            "longParams": report.long_params_count,
            "todo": report.to_todo_list(),
        }))
        .unwrap()
    } else {
        let mut output = format!("HEALTH SCORE: {}/100 — \"{}\"\n", report.score, report.label);
        output.push_str("  Todo:\n");
        for todo in report.to_todo_list() {
            output.push_str(&format!("    • {}\n", todo));
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_100_for_clean() {
        let graph = crate::graph::DepGraph::new(Vec::new());
        let weights = crate::config::Weights::default();
        let root = std::path::Path::new(".");
        let report = calculate(&graph, &[], &[], &weights, root);
        assert_eq!(report.score, 100);
        assert_eq!(report.label, "healthy");
    }

    #[test]
    fn test_score_floors_at_0() {
        let graph = crate::graph::DepGraph::new(Vec::new());
        let weights = crate::config::Weights::default();
        let root = std::path::Path::new(".");
        let giant_metrics = vec![
            giant_funcs::FunctionMetrics {
                file_path: "x.ts".into(),
                name: "f".into(),
                start_line: 1,
                end_line: 100,
                logical_lines: 50,
                complexity: 30,
                severity: Severity::Critical,
            },
        ];
        let risk_items = (0..100)
            .map(|_| risks::RiskItem {
                kind: RiskKind::AnyType,
                file_path: "x.ts".into(),
                line: 1,
                detail: "".into(),
            })
            .collect::<Vec<_>>();
        let report = calculate(&graph, &giant_metrics, &risk_items, &weights, root);
        assert_eq!(report.score, 0, "should floor at 0");
    }

    #[test]
    fn test_health_on_fixtures() {
        let root = std::path::Path::new("../../test-fixtures/vibe-app");
        let config = crate::config::DelveConfig::default();
        let output = run_health(root, false, &config);
        assert!(output.contains("HEALTH SCORE"), "should have health score");
        assert!(output.contains("Todo"), "should have todo list");
    }
}
