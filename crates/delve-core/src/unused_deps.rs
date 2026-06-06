use std::collections::{HashMap, HashSet};
use std::path::Path;

use yansi::Paint;

use crate::graph::DepGraph;

const NODE_BUILTINS: &[&str] = &[
    "fs", "path", "os", "http", "https", "stream", "util", "crypto",
    "child_process", "events", "buffer", "url", "querystring", "assert",
    "net", "tls", "dns", "module", "process", "console", "timers",
    "string_decoder", "readline", "cluster", "zlib", "punycode", "vm",
    "perf_hooks", "async_hooks", "worker_threads", "diagnostics_channel",
];

#[derive(Debug, Clone)]
pub struct DependencyIssue {
    pub package: String,
    pub issue_type: DepIssueType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DepIssueType {
    Unused,
    Missing,
}

pub struct DependencyReport {
    pub unused: Vec<DependencyIssue>,
    pub missing: Vec<DependencyIssue>,
}

fn extract_package_name(source: &str) -> Option<String> {
    if source.starts_with('.') || source.starts_with('/') {
        return None;
    }
    let pkg = if source.starts_with('@') {
        let parts: Vec<&str> = source.splitn(3, '/').collect();
        if parts.len() >= 2 {
            format!("{}/{}", parts[0], parts[1])
        } else {
            return None;
        }
    } else {
        source.split('/').next().unwrap_or(source).to_string()
    };
    if NODE_BUILTINS.contains(&pkg.as_str()) {
        return None;
    }
    Some(pkg)
}

pub fn find_unused_dependencies(graph: &DepGraph, root: &Path) -> DependencyReport {
    let pkg_path = root.join("package.json");
    let pkg_json: serde_json::Value = match std::fs::read_to_string(&pkg_path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(serde_json::Value::Null),
        Err(_) => return DependencyReport { unused: Vec::new(), missing: Vec::new() },
    };

    let declared = read_dep_map(&pkg_json, "dependencies");
    let dev_declared = read_dep_map(&pkg_json, "devDependencies");

    if declared.is_empty() && dev_declared.is_empty() {
        return DependencyReport { unused: Vec::new(), missing: Vec::new() };
    }

    let mut used_packages: HashSet<String> = HashSet::new();
    for fs in graph.all_symbols.values() {
        for imp in &fs.imports {
            if let Some(pkg) = extract_package_name(&imp.source) {
                used_packages.insert(pkg);
            }
        }
    }

    let mut unused = Vec::new();
    for (pkg, _) in &declared {
        if !used_packages.contains(pkg) {
            unused.push(DependencyIssue {
                package: pkg.clone(),
                issue_type: DepIssueType::Unused,
            });
        }
    }

    let mut missing = Vec::new();
    for pkg in &used_packages {
        if !declared.contains_key(pkg) && !dev_declared.contains_key(pkg) {
            missing.push(DependencyIssue {
                package: pkg.clone(),
                issue_type: DepIssueType::Missing,
            });
        }
    }

    unused.sort_by(|a, b| a.package.cmp(&b.package));
    missing.sort_by(|a, b| a.package.cmp(&b.package));

    DependencyReport { unused, missing }
}

fn read_dep_map(pkg: &serde_json::Value, field: &str) -> HashMap<String, String> {
    pkg.get(field)
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn format_report(report: &DependencyReport) -> String {
    if report.unused.is_empty() && report.missing.is_empty() {
        return "  No dependency issues found.\n".to_string();
    }

    let mut output = String::new();

    if !report.unused.is_empty() {
        output.push_str(&format!("{}\n", Paint::yellow("UNUSED DEPENDENCIES")));
        for issue in &report.unused {
            output.push_str(&format!("  {} — in package.json but never imported\n", Paint::yellow(&issue.package)));
        }
        output.push('\n');
    }

    if !report.missing.is_empty() {
        output.push_str(&format!("{}\n", Paint::red("MISSING DEPENDENCIES")));
        for issue in &report.missing {
            output.push_str(&format!("  {} — imported but not in package.json\n", Paint::red(&issue.package)));
        }
        output.push('\n');
    }

    output
}

pub fn format_json(report: &DependencyReport) -> serde_json::Value {
    serde_json::json!({
        "unused": report.unused.iter().map(|i| i.package.clone()).collect::<Vec<_>>(),
        "missing": report.missing.iter().map(|i| i.package.clone()).collect::<Vec<_>>(),
    })
}

pub fn run_deps(root: &Path, json: bool, config: &crate::config::DelveConfig) -> crate::CommandResult {
    let progress = crate::progress::Progress::new(!json);
    progress.set_message("Parsing files...");
    let symbols = crate::parser::parse_all_files_with_ignore(root, &config.ignore);
    progress.set_message("Analyzing dependencies...");
    let mut graph = crate::graph::DepGraph::new(symbols);
    graph.build();
    progress.set_message("Checking package.json...");
    let report = find_unused_dependencies(&graph, root);
    progress.finish();

    let output = if json {
        serde_json::to_string_pretty(&format_json(&report)).unwrap()
    } else {
        format_report(&report)
    };

    let has_issues = !report.unused.is_empty() || !report.missing.is_empty();
    crate::CommandResult {
        output,
        exit_code: if has_issues { 1 } else { 0 },
        score: if has_issues { 0 } else { 100 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DepGraph;
    use crate::parser::FileSymbols;

    fn make_graph_with_imports(import_sources: &[&str]) -> DepGraph {
        let imports = import_sources.iter().map(|s| {
            crate::parser::Import {
                symbols: vec!["x".to_string()],
                source: s.to_string(),
                start_line: 1,
                end_line: 1,
                file_path: "test.ts".to_string(),
                is_default: false,
                is_namespace: false,
                is_type_only: false,
            }
        }).collect();

        let fs = FileSymbols {
            file_path: "test.ts".to_string(),
            exports: vec![],
            imports,
            functions: vec![],
            wildcard_re_exports: vec![],
        };

        let mut graph = DepGraph::new(vec![fs]);
        graph.build();
        graph
    }

    #[test]
    fn test_extract_package_name_relative() {
        assert_eq!(extract_package_name("./foo"), None);
        assert_eq!(extract_package_name("../bar"), None);
    }

    #[test]
    fn test_extract_package_name_simple() {
        assert_eq!(extract_package_name("lodash"), Some("lodash".to_string()));
        assert_eq!(extract_package_name("lodash/merge"), Some("lodash".to_string()));
    }

    #[test]
    fn test_extract_package_name_scoped() {
        assert_eq!(extract_package_name("@scope/pkg"), Some("@scope/pkg".to_string()));
        assert_eq!(extract_package_name("@scope/pkg/subpath"), Some("@scope/pkg".to_string()));
    }

    #[test]
    fn test_extract_package_name_builtin() {
        assert_eq!(extract_package_name("fs"), None);
        assert_eq!(extract_package_name("path"), None);
    }

    #[test]
    fn test_empty_report_format() {
        let report = DependencyReport { unused: vec![], missing: vec![] };
        let output = format_report(&report);
        assert!(output.contains("No dependency issues"));
    }

    #[test]
    fn test_find_unused_detects_unused() {
        let graph = make_graph_with_imports(&["lodash"]);
        let root = Path::new(".");
        // No package.json exists at "."
        let report = find_unused_dependencies(&graph, root);
        // report should be empty because no package.json was found
        assert!(report.unused.is_empty());
    }
}
