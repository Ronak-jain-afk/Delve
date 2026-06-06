use std::path::Path;

use yansi::Paint;

use crate::duplicates;
use crate::giant_funcs;
use crate::graph::DepGraph;
use crate::risks;
use crate::unused;

fn maybe_add_annotations(output: &mut String, annotations: bool, prefix: &str, items: &[unused::UnusedItem]) {
    if !annotations {
        return;
    }
    for item in items {
        let file = item.file_path.replace('\\', "/");
        output.push_str(&format!("::warning file={},line={},title={}::{} is {} and never imported\n", file, item.line, prefix, item.symbol, item.kind));
    }
}

pub fn run_full_audit(root: &Path, json: bool, sarif: bool, annotations: bool, config: &crate::config::DelveConfig) -> crate::CommandResult {
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
    let file_tokens = duplicates::tokenize_files(&files);
    let dup_clusters = duplicates::find_duplicates(&file_tokens);

    let unused_items = unused::find_unused(&graph);
    let health = crate::health::calculate(&graph, &giant_metrics, &risk_items, &config.weights, root);

    if sarif {
        progress.finish();
        let mut results = Vec::new();

        for item in &unused_items {
            let file_uri = format!("{}", std::path::Path::new(&item.file_path).canonicalize().ok().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| item.file_path.clone()));
            results.push(serde_json::json!({
                "ruleId": "delve/unused-export",
                "level": "warning",
                "message": { "text": format!("{} is exported {}, never imported", item.symbol, item.kind) },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": file_uri },
                        "region": { "startLine": item.line }
                    }
                }]
            }));
        }

        for m in &giant_metrics {
            let sev = match m.severity {
                giant_funcs::Severity::Critical => "error",
                giant_funcs::Severity::Warning => "warning",
            };
            results.push(serde_json::json!({
                "ruleId": "delve/giant-function",
                "level": sev,
                "message": { "text": format!("{} ({} lines, complexity {})", m.name, m.logical_lines, m.complexity) },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": format!("file://{}", m.file_path) },
                        "region": { "startLine": m.start_line }
                    }
                }]
            }));
        }

        for cluster in &dup_clusters {
            if let Some(loc) = cluster.locations.first() {
                results.push(serde_json::json!({
                    "ruleId": "delve/duplicate-block",
                    "level": "warning",
                    "message": { "text": format!("Duplicate block ({} tokens) found in {} locations", cluster.token_count, cluster.locations.len()) },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": format!("file://{}", loc.file_path) },
                            "region": { "startLine": loc.start_line }
                        }
                    }]
                }));
            }
        }

        for risk in &risk_items {
            let (rule_id, level) = match risk.kind {
                crate::risks::RiskKind::AnyType => ("delve/any-type", "warning"),
                crate::risks::RiskKind::ConsoleLog => ("delve/console-log", "warning"),
                crate::risks::RiskKind::Debugger => ("delve/debugger", "error"),
                crate::risks::RiskKind::DeepNesting => ("delve/deep-nesting", "warning"),
                crate::risks::RiskKind::LongParams => ("delve/long-params", "warning"),
            };
            results.push(serde_json::json!({
                "ruleId": rule_id,
                "level": level,
                "message": { "text": &risk.detail },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": format!("file://{}", risk.file_path) },
                        "region": { "startLine": risk.line }
                    }
                }]
            }));
        }

        let output = serde_json::to_string_pretty(&serde_json::json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "delve",
                        "informationUri": "https://github.com/Ronak-jain-afk/Delve",
                        "version": "0.1.1"
                    }
                },
                "results": results,
                "properties": {
                    "healthScore": health.score,
                    "circularDependencies": graph.find_circular_dependencies().len()
                }
            }]
        })).unwrap();
        let exit_code = if health.score >= 70 { 0 } else if health.score >= 40 { 1 } else { 2 };
        return crate::CommandResult { output, exit_code, score: health.score };
    }

    if json {
        progress.finish();
        let output = serde_json::to_string_pretty(&serde_json::json!({
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
            "circularDependencies": graph.find_circular_dependencies().iter().map(|cycle| {
                serde_json::json!(cycle)
            }).collect::<Vec<_>>(),
        }))
        .unwrap();
        let exit_code = if health.score >= 70 { 0 } else if health.score >= 40 { 1 } else { 2 };
        return crate::CommandResult { output, exit_code, score: health.score };
    } else {
        let mut output = format!("{}\n\n", Paint::bold(&format!("Delve Audit — {}", root.display())));

        let unused_items = unused::find_unused(&graph);
        if unused_items.is_empty() {
            output.push_str(&format!("{}\n  No unused exports found.\n\n", Paint::yellow("UNUSED CODE")));
        } else {
            output.push_str(&unused::format_unused_report(&unused_items));
            maybe_add_annotations(&mut output, annotations, "delve-unused", &unused_items);
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

        let cycles = graph.find_circular_dependencies();
        if cycles.is_empty() {
            output.push_str(&format!("{}\n  No circular dependencies found.\n\n", Paint::yellow("CIRCULAR DEPENDENCIES")));
        } else {
            output.push_str(&format!("{}\n", Paint::yellow("CIRCULAR DEPENDENCIES")));
            for cycle in &cycles {
                output.push_str(&format!("  {} → {} → {}\n", Paint::red("CYCLE"), cycle.join(" → "), Paint::red(&cycle[0])));
            }
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
        let exit_code = if health.score >= 70 { 0 } else if health.score >= 40 { 1 } else { 2 };
        crate::CommandResult { output, exit_code, score: health.score }
    }
}
