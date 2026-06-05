use std::path::Path;

use yansi::Paint;

use crate::duplicates;
use crate::giant_funcs;
use crate::graph::DepGraph;
use crate::risks;
use crate::unused;

pub fn run_full_audit(root: &Path, json: bool, config: &crate::config::DelveConfig) -> String {
    let progress = crate::progress::Progress::new(!json);

    // Parse once, reuse everywhere
    progress.set_message("Parsing files...");
    let symbols = crate::parser::parse_all_files_with_ignore(root, &config.ignore);

    progress.set_message("Analyzing dependencies...");
    let mut graph = DepGraph::new(symbols);
    graph.build();
    graph.detect_entry_points();
    graph.traverse_from_entry_points();

    progress.set_message("Analyzing giant functions...");
    let all_symbols: Vec<_> = graph.all_symbols.values().cloned().collect();
    let giant_metrics = giant_funcs::analyze_functions(&all_symbols, &config.thresholds);

    progress.set_message("Detecting risky patterns...");
    let risk_items = risks::detect_risks_with_ignore(root, &config.ignore);

    progress.set_message("Detecting duplicates...");
    let files = crate::parser::find_source_files_with_ignore(root, &config.ignore);
    let dup_clusters = duplicates::find_duplicates(&files);

    if json {
        progress.finish();
        let unused_items = unused::find_unused(&graph);
        let health = crate::health::calculate(&graph, &giant_metrics, &risk_items, &config.weights, root);

        serde_json::to_string_pretty(&serde_json::json!({
            "score": health.score,
            "unused": unused::format_unused_json(&unused_items),
            "giantFunctions": giant_funcs::format_json(&giant_metrics),
            "duplicates": dup_clusters.iter().map(|c| {
                serde_json::json!({
                    "sample": c.sample,
                    "tokenCount": c.token_count,
                    "locations": c.locations.iter().map(|l| {
                        format!("{}", l.file_path)
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
            "risks": risks::format_json(&risk_items),
        }))
        .unwrap()
    } else {
        let mut output = format!("{}\n\n", Paint::bold(&format!("Delve Audit — {}", root.display())));

        let unused_items = unused::find_unused(&graph);
        if unused_items.is_empty() {
            output.push_str(&format!("{}\n  No unused exports found.\n\n", Paint::yellow("UNUSED CODE")));
        } else {
            output.push_str(&unused::format_unused_report(&unused_items));
            output.push('\n');
        }

        if giant_metrics.is_empty() {
            output.push_str(&format!("{}\n  No giant functions found.\n\n", Paint::yellow("GIANT FUNCTIONS")));
        } else {
            output.push_str(&giant_funcs::format_report(&giant_metrics));
            output.push('\n');
        }

        if dup_clusters.is_empty() {
            output.push_str(&format!("{}\n  No duplicate blocks found.\n\n", Paint::yellow("DUPLICATE BLOCKS")));
        } else {
            output.push_str(&duplicates::format_report(&dup_clusters));
            output.push('\n');
        }

        if risk_items.is_empty() {
            output.push_str(&format!("{}\n  No risky patterns found.\n\n", Paint::yellow("RISKY PATTERNS")));
        } else {
            output.push_str(&risks::format_report(&risk_items));
            output.push('\n');
        }

        let health = crate::health::calculate(&graph, &giant_metrics, &risk_items, &config.weights, root);
        let colored_label = match health.label {
            "healthy" => Paint::green(health.label).to_string(),
            "needs work" => Paint::yellow(health.label).to_string(),
            _ => Paint::red(health.label).to_string(),
        };
        output.push_str(&format!("HEALTH SCORE: {}/100 — \"{}\"\n", Paint::bold(&health.score.to_string()), colored_label));
        for todo in health.to_todo_list() {
            output.push_str(&format!("  → {}\n", todo));
        }

        progress.finish();
        output
    }
}
