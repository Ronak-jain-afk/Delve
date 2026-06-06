use std::path::Path;

use yansi::Paint;

use crate::graph::DepGraph;
use crate::parser::ExportKind;

pub struct UnusedItem {
    pub file_path: String,
    pub symbol: String,
    pub line: usize,
    pub kind: String,
}

pub fn find_unused(graph: &DepGraph) -> Vec<UnusedItem> {
    let mut items = Vec::new();
    for (file_path, symbols) in &graph.all_symbols {
        for exp in &symbols.exports {
            if exp.re_export_source.is_some() {
                continue; // Barrels are pass-through; the source export is what matters
            }
            if !graph.reachable_exports.contains(&(file_path.clone(), exp.name.clone())) {
                // Check for /* delve:used */ comment
                if !has_delve_used_comment(file_path, exp.start_line) {
                    items.push(UnusedItem {
                        file_path: file_path.clone(),
                        symbol: exp.name.clone(),
                        line: exp.start_line,
                        kind: export_kind_string(&exp.kind),
                    });
                }
            }
        }
    }
    items.sort_by(|a, b| a.file_path.cmp(&b.file_path).then(a.line.cmp(&b.line)));
    items
}

fn has_delve_used_comment(file_path: &str, line: usize) -> bool {
    if let Ok(content) = std::fs::read_to_string(file_path) {
        let lines: Vec<&str> = content.lines().collect();
        // Check the line before the export
        if line > 0 {
            if let Some(prev_line) = lines.get(line.saturating_sub(2)) {
                if prev_line.trim().contains("/* delve:used */")
                    || prev_line.trim().contains("// delve:used")
                {
                    return true;
                }
            }
        }
    }
    false
}

fn export_kind_string(kind: &ExportKind) -> String {
    match kind {
        ExportKind::Function => "function".to_string(),
        ExportKind::Const => "const".to_string(),
        ExportKind::Class => "class".to_string(),
        ExportKind::Interface => "interface".to_string(),
        ExportKind::Type => "type".to_string(),
        ExportKind::Default => "default".to_string(),
        ExportKind::Named => "named".to_string(),
    }
}

pub fn format_unused_report(items: &[UnusedItem]) -> String {
    if items.is_empty() {
        return "  No unused exports found.\n".to_string();
    }
    let mut output = format!("{}\n", Paint::yellow("UNUSED CODE (safe to delete)"));
    for item in items {
        output.push_str(&format!(
            "  {}:{}   {} (exported {}, never imported)\n",
            item.file_path, item.line, item.symbol, item.kind
        ));
    }
    output
}

pub fn format_unused_json(items: &[UnusedItem]) -> serde_json::Value {
    serde_json::json!(items.iter().map(|item| {
        serde_json::json!({
            "file": item.file_path,
            "symbol": item.symbol,
            "line": item.line,
            "kind": item.kind,
        })
    }).collect::<Vec<_>>())
}

pub fn run_deadcode(root: &Path, json: bool, config: &crate::config::DelveConfig) -> String {
    let progress = crate::progress::Progress::new(!json);
    progress.set_message("Parsing files...");
    let symbols = crate::parser::parse_all_files_with_ignore(root, &config.ignore);
    progress.set_message("Analyzing dependencies...");
    let mut graph = crate::graph::DepGraph::new(symbols);
    graph.build();
    graph.detect_entry_points();
    graph.traverse_from_entry_points();
    progress.set_message("Finding unused exports...");
    let items = find_unused(&graph);
    progress.finish();
    if json {
        serde_json::to_string_pretty(&format_unused_json(&items)).unwrap()
    } else {
        format_unused_report(&items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_delve_used_comment() {
        let path = std::fs::canonicalize("../../test-fixtures/vibe-app/src/utils/formatDate.ts")
            .unwrap()
            .to_string_lossy()
            .to_string();
        // oldHelper function starts on line 11; comment /* delve:used */ is on line 10
        assert!(has_delve_used_comment(&path, 11), "oldHelper should have delve:used comment above it");
        // formatTimestamp starts on line 1
        assert!(!has_delve_used_comment(&path, 1), "formatTimestamp should not have delve:used comment");
    }
}
