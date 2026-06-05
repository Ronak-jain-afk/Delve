use std::path::Path;

use crate::parser;

#[derive(Debug, Clone)]
pub struct RiskItem {
    pub kind: RiskKind,
    pub file_path: String,
    pub line: usize,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RiskKind {
    AnyType,
    ConsoleLog,
    Debugger,
    DeepNesting,
    LongParams,
}

pub fn detect_risks(root: &Path) -> Vec<RiskItem> {
    let mut risks = Vec::new();
    let files = parser::find_source_files(root);

    for file_path in &files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let is_test_file = file_path.contains("__tests__")
            || file_path.contains(".test.")
            || file_path.contains(".spec.");

        let is_typescript = file_path.ends_with(".ts") || file_path.ends_with(".tsx");

        for (line_num, line) in source.lines().enumerate() {
            let line_num = line_num + 1;
            let trimmed = line.trim();

            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*") {
                continue;
            }

            // any type detection (TypeScript only)
            if is_typescript && detect_any_type(&source, line, trimmed) {
                risks.push(RiskItem {
                    kind: RiskKind::AnyType,
                    file_path: file_path.clone(),
                    line: line_num,
                    detail: "any type used".to_string(),
                });
            }

            // console.log detection (skip test files)
            if !is_test_file && detect_console_log(trimmed) {
                risks.push(RiskItem {
                    kind: RiskKind::ConsoleLog,
                    file_path: file_path.clone(),
                    line: line_num,
                    detail: "console.log left in production".to_string(),
                });
            }

            // debugger detection
            if !is_test_file && trimmed == "debugger;" || trimmed == "debugger" {
                risks.push(RiskItem {
                    kind: RiskKind::Debugger,
                    file_path: file_path.clone(),
                    line: line_num,
                    detail: "debugger statement".to_string(),
                });
            }
        }

        // Deep nesting detection
        detect_deep_nesting(&source, file_path, &mut risks);

        // Long parameter list detection
        detect_long_params(&source, file_path, &mut risks);
    }

    risks.sort_by(|a, b| a.file_path.cmp(&b.file_path).then(a.line.cmp(&b.line)));
    risks
}

fn detect_any_type(_source: &str, _line: &str, trimmed: &str) -> bool {
    if trimmed.contains(": any") || trimmed.contains("as any") {
        if let Some(comment_pos) = trimmed.find("//") {
            let before_comment = &trimmed[..comment_pos];
            before_comment.contains(": any") || before_comment.contains("as any")
        } else {
            true
        }
    } else {
        false
    }
}

fn detect_console_log(trimmed: &str) -> bool {
    // Remove inline comments before checking
    let code = if let Some(pos) = trimmed.find("//") {
        &trimmed[..pos]
    } else {
        trimmed
    };
    code.contains("console.log(") || code.contains("console.log (")
}

fn detect_deep_nesting(source: &str, file_path: &str, risks: &mut Vec<RiskItem>) {
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut max_depth_line: usize = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments and strings (simplified)
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*") {
            continue;
        }

        for ch in trimmed.chars() {
            if ch == '{' {
                depth += 1;
                if depth > max_depth {
                    max_depth = depth;
                    max_depth_line = line_num + 1;
                }
            } else if ch == '}' {
                depth = depth.saturating_sub(1);
            }
        }
    }

    if max_depth > 4 {
        risks.push(RiskItem {
            kind: RiskKind::DeepNesting,
            file_path: file_path.to_string(),
            line: max_depth_line,
            detail: format!("nesting depth {} (limit: 4)", max_depth),
        });
    }
}

fn detect_long_params(source: &str, file_path: &str, risks: &mut Vec<RiskItem>) {
    // Simple regex-free approach: look for function(...) patterns with many commas in the param list
    let mut in_function = false;
    let mut param_count = 0;
    let mut fn_line = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("function ") || trimmed.contains("= function(") || trimmed.contains("= (") {
            in_function = true;
            param_count = 0;
            fn_line = line_num + 1;

            // Count params in the function declaration
            if let Some(start) = trimmed.find('(') {
                if let Some(end) = trimmed[start..].find(')') {
                    let params = &trimmed[start + 1..start + end];
                    if !params.is_empty() {
                        param_count = params.split(',').count();
                    }
                }
            }
        }

        // Arrow functions: const foo = (a, b, c) =>
        if trimmed.contains("=>") && !in_function {
            if let Some(start) = trimmed.find('(') {
                if let Some(end) = trimmed[start..].find(')') {
                    let params = &trimmed[start + 1..start + end];
                    if !params.is_empty() {
                        param_count = params.split(',').count();
                        fn_line = line_num + 1;
                    }
                }
            }
        }

        if in_function && param_count > 5 {
            risks.push(RiskItem {
                kind: RiskKind::LongParams,
                file_path: file_path.to_string(),
                line: fn_line,
                detail: format!("{} parameters (limit: 5)", param_count),
            });
            in_function = false;
        }
    }
}

pub fn format_report(risks: &[RiskItem]) -> String {
    if risks.is_empty() {
        return "  No risky patterns found.\n".to_string();
    }
    let mut output = String::from("RISKY PATTERNS\n");
    for item in risks {
        let label = match item.kind {
            RiskKind::AnyType => "any type",
            RiskKind::ConsoleLog => "console.log",
            RiskKind::Debugger => "debugger",
            RiskKind::DeepNesting => "deep nesting",
            RiskKind::LongParams => "too many parameters",
        };
        output.push_str(&format!(
            "  {}:{}   {} ({})\n",
            item.file_path, item.line, label, item.detail
        ));
    }
    output
}

pub fn format_json(risks: &[RiskItem]) -> serde_json::Value {
    let any_count = risks.iter().filter(|r| r.kind == RiskKind::AnyType).count();
    let log_count = risks.iter().filter(|r| r.kind == RiskKind::ConsoleLog).count();
    let debugger_count = risks.iter().filter(|r| r.kind == RiskKind::Debugger).count();
    let nesting_count = risks.iter().filter(|r| r.kind == RiskKind::DeepNesting).count();
    let params_count = risks.iter().filter(|r| r.kind == RiskKind::LongParams).count();

    serde_json::json!({
        "anyTypes": any_count,
        "consoleLogs": log_count,
        "debuggerStatements": debugger_count,
        "deepNesting": nesting_count,
        "longParams": params_count,
        "items": risks.iter().map(|r| {
            serde_json::json!({
                "file": r.file_path,
                "line": r.line,
                "kind": format!("{:?}", r.kind),
                "detail": r.detail,
            })
        }).collect::<Vec<_>>()
    })
}

pub fn run_risks(root: &Path) -> Vec<RiskItem> {
    detect_risks(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_console_log() {
        assert!(detect_console_log("console.log('hello');"));
        assert!(!detect_console_log("const x = 1;"));
        assert!(!detect_console_log("// console.log('test');"));
    }

    #[test]
    fn test_detect_any_type() {
        let source = "let x: any = 1;\nconst y = foo as any;\nlet z: string = 'hello';";
        assert!(detect_any_type(source, "let x: any = 1;", "let x: any = 1;"));
        assert!(detect_any_type(source, "const y = foo as any;", "const y = foo as any;"));
        assert!(!detect_any_type(source, "let z: string = 'hello';", "let z: string = 'hello';"));
    }

    #[test]
    fn test_detect_deep_nesting() {
        let source = "{\n  {\n    {\n      {\n        {\n          // depth 5\n        }\n      }\n    }\n  }\n}";
        let file = "test.ts";
        let mut risks = Vec::new();
        detect_deep_nesting(source, file, &mut risks);
        assert!(!risks.is_empty(), "should detect deep nesting");
    }

    #[test]
    fn test_risks_on_fixtures() {
        let root = std::path::Path::new("../../test-fixtures/vibe-app");
        let risks = detect_risks(root);
        let has_any = risks.iter().any(|r| r.kind == RiskKind::AnyType);
        let has_log = risks.iter().any(|r| r.kind == RiskKind::ConsoleLog);
        assert!(has_any, "Dashboard.tsx has any types");
        assert!(has_log, "index.ts has console.log");
    }
}
