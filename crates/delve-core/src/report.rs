use std::path::Path;

use crate::duplicates;
use crate::giant_funcs;
use crate::graph::DepGraph;
use crate::risks;
use crate::unused;

pub fn run_full_audit(root: &Path, json: bool) -> String {
    // Run all analysis passes
    let symbols = crate::parser::parse_all_files(root);
    let mut graph = DepGraph::new(symbols);
    graph.build();
    graph.detect_entry_points();
    graph.traverse_from_entry_points();

    let all_symbols = crate::parser::parse_all_files(root);
    let giant_metrics = giant_funcs::analyze_functions(&all_symbols);
    let risk_items = risks::detect_risks(root);
    let files = crate::parser::find_source_files(root);
    let dup_clusters = duplicates::find_duplicates(&files);

    if json {
        let unused_items = unused::find_unused(&graph);
        let health = crate::health::calculate(&graph, &giant_metrics, &risk_items);

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
        let mut output = format!("Delve Audit — {}\n\n", root.display());

        // Unused code section
        let unused_items = unused::find_unused(&graph);
        if unused_items.is_empty() {
            output.push_str("UNUSED CODE\n  No unused exports found.\n\n");
        } else {
            output.push_str(&unused::format_unused_report(&unused_items));
            output.push('\n');
        }

        // Giant functions section
        if giant_metrics.is_empty() {
            output.push_str("GIANT FUNCTIONS\n  No giant functions found.\n\n");
        } else {
            output.push_str(&giant_funcs::format_report(&giant_metrics));
            output.push('\n');
        }

        // Duplicates section
        if dup_clusters.is_empty() {
            output.push_str("DUPLICATE BLOCKS\n  No duplicate blocks found.\n\n");
        } else {
            output.push_str(&duplicates::format_report(&dup_clusters));
            output.push('\n');
        }

        // Risks section
        if risk_items.is_empty() {
            output.push_str("RISKY PATTERNS\n  No risky patterns found.\n\n");
        } else {
            output.push_str(&risks::format_report(&risk_items));
            output.push('\n');
        }

        // Health score
        let health = crate::health::calculate(&graph, &giant_metrics, &risk_items);
        output.push_str(&format!("HEALTH SCORE: {}/100 — \"{}\"\n", health.score, health.label));
        for todo in health.to_todo_list() {
            output.push_str(&format!("  → {}\n", todo));
        }

        output
    }
}
