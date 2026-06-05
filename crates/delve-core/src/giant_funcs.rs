use std::path::Path;

use crate::config::Thresholds;
use crate::parser::FileSymbols;

pub struct FunctionMetrics {
    pub file_path: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub logical_lines: usize,
    pub complexity: usize,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Warning,
    Critical,
}

pub fn count_logical_lines(source: &str, start_byte: usize, end_byte: usize) -> usize {
    let body = &source[start_byte..end_byte];
    let mut count = 0;
    let mut in_block_comment = false;

    for line in body.lines() {
        let trimmed = line.trim();

        // Skip blank lines
        if trimmed.is_empty() {
            continue;
        }

        // Handle block comments
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        // Skip line comments
        if trimmed.starts_with("//") {
            continue;
        }

        // Skip bare braces
        if trimmed == "{" || trimmed == "}" {
            continue;
        }

        count += 1;
    }

    count.max(1) // At least 1 line
}

pub fn compute_complexity(source: &str, start_byte: usize, end_byte: usize) -> usize {
    let body = &source[start_byte..end_byte];
    let mut complexity = 1; // Base complexity
    let mut chars = body.char_indices().peekable();
    let mut in_block_comment = false;
    let mut in_line_comment = false;
    let mut in_string = false;
    let mut string_char = '"';

    while let Some((i, c)) = chars.next() {
        if in_block_comment {
            if c == '*' && chars.peek().map(|(_, c)| *c) == Some('/') {
                in_block_comment = false;
                chars.next();
            }
            continue;
        }
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_string {
            if c == '\\' {
                chars.next();
                continue;
            }
            if c == string_char {
                in_string = false;
            }
            continue;
        }

        match c {
            '/' if chars.peek().map(|(_, c)| *c) == Some('*') => {
                in_block_comment = true;
                chars.next();
            }
            '/' if chars.peek().map(|(_, c)| *c) == Some('/') => {
                in_line_comment = true;
                chars.next();
            }
            '"' | '\'' | '`' => {
                in_string = true;
                string_char = c;
            }
            _ => {}
        }

        // Check for control flow patterns
        if body[i..].starts_with("if ") || body[i..].starts_with("if(") {
            complexity += 1;
        }
        if body[i..].starts_with("else if ") || body[i..].starts_with("else if(") {
            // Don't double-count — else if is already counted as an `if`
            // Actually, `else if` should increment, but our `if` check above catches it
        }
        if body[i..].starts_with("for ") || body[i..].starts_with("for(")
            || body[i..].starts_with("for await ")
        {
            complexity += 1;
        }
        if body[i..].starts_with("while ") || body[i..].starts_with("while(") {
            complexity += 1;
        }
        if body[i..].starts_with("do ") || body[i..].starts_with("do{") {
            complexity += 1;
        }
        if body[i..].starts_with("case ") {
            complexity += 1;
        }
        if body[i..].starts_with("catch ") || body[i..].starts_with("catch(") {
            complexity += 1;
        }
        // Ternary
        if c == '?' && !body[i..].starts_with("??") && !body[i..].starts_with("?.") && !body[i..].starts_with("?:") {
            // Check it's a ternary, not optional chaining or nullish coalescing
            let remaining = &body[i..];
            if let Some(next_after_q) = remaining.chars().nth(1) {
                if next_after_q != '?' && next_after_q != '.' {
                    complexity += 1;
                }
            }
        }
        // && and ||
        if body[i..].starts_with("&&") {
            complexity += 1;
        }
        if body[i..].starts_with("||") {
            complexity += 1;
        }
    }

    complexity
}

pub fn analyze_functions(symbols: &[FileSymbols], thresholds: &Thresholds) -> Vec<FunctionMetrics> {
    let mut all_metrics = Vec::new();

    for file_sym in symbols {
        for func in &file_sym.functions {
            let source = match std::fs::read_to_string(&file_sym.file_path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let logical_lines = count_logical_lines(&source, func.start_byte, func.end_byte);
            let complexity = compute_complexity(&source, func.start_byte, func.end_byte);

            let severity = if logical_lines > thresholds.critical_lines || complexity > thresholds.critical_complexity {
                Severity::Critical
            } else if logical_lines > thresholds.warning_lines || complexity > thresholds.warning_complexity {
                Severity::Warning
            } else {
                continue;
            };

            all_metrics.push(FunctionMetrics {
                file_path: file_sym.file_path.clone(),
                name: func.name.clone().unwrap_or_else(|| "<anonymous>".to_string()),
                start_line: func.start_line,
                end_line: func.end_line,
                logical_lines,
                complexity,
                severity,
            });
        }
    }

    all_metrics.sort_by(|a, b| a.file_path.cmp(&b.file_path).then(a.start_line.cmp(&b.start_line)));
    all_metrics
}

pub fn format_report(metrics: &[FunctionMetrics]) -> String {
    if metrics.is_empty() {
        return "  No giant functions found.\n".to_string();
    }
    let mut output = String::from("GIANT FUNCTIONS (split me)\n");
    for m in metrics {
        let label = match m.severity {
            Severity::Critical => "CRITICAL",
            Severity::Warning => "warning",
        };
        output.push_str(&format!(
            "  [{}] {}:{}   {} ({} lines, complexity {})\n",
            label, m.file_path, m.start_line, m.name, m.logical_lines, m.complexity
        ));
    }
    output
}

pub fn format_json(metrics: &[FunctionMetrics]) -> serde_json::Value {
    serde_json::json!(metrics.iter().map(|m| {
        serde_json::json!({
            "file": m.file_path,
            "name": m.name,
            "startLine": m.start_line,
            "endLine": m.end_line,
            "lines": m.logical_lines,
            "complexity": m.complexity,
            "severity": match m.severity {
                Severity::Critical => "critical",
                Severity::Warning => "warning",
            },
        })
    }).collect::<Vec<_>>())
}

pub fn run_split(root: &Path, json: bool, config: &crate::config::DelveConfig) -> String {
    let symbols = crate::parser::parse_all_files_with_ignore(root, &config.ignore);
    let metrics = analyze_functions(&symbols, &config.thresholds);
    if json {
        serde_json::to_string_pretty(&format_json(&metrics)).unwrap()
    } else {
        format_report(&metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_logical_lines() {
        let source = "function foo() {\n  // comment\n  \n  return 1;\n}\n";
        let count = count_logical_lines(source, 0, source.len());
        assert_eq!(count, 2, "should count function declaration + return, skip comment and blank");
    }

    #[test]
    fn test_compute_complexity_base() {
        let source = "function foo() { return 1; }";
        let c = compute_complexity(source, 0, source.len());
        assert_eq!(c, 1, "base complexity is 1");
    }

    #[test]
    fn test_compute_complexity_with_if() {
        let source = "function foo(x) { if (x) { return 1; } }";
        let c = compute_complexity(source, 0, source.len());
        assert_eq!(c, 2, "1 if = complexity 2");
    }

    #[test]
    fn test_compute_complexity_with_logical_ops() {
        let source = "function foo(x, y) { return x && y || z; }";
        let c = compute_complexity(source, 0, source.len());
        assert_eq!(c, 3, "&& and || = +2");
    }

    #[test]
    fn test_analyze_simple_complexity() {
        let source = "function complex(x, y, z) { if (x) { if (y) { return z; } } for(let i=0;i<10;i++){} return x && y || z; }";
        let byte_end = source.len();
        let c = compute_complexity(source, 0, byte_end);
        assert!(c > 1, "complex function should have complexity > 1, got {}", c);
    }

    #[test]
    fn test_analyze_returns_metrics() {
        let root = std::path::Path::new("../../test-fixtures/vibe-app");
        let symbols = crate::parser::parse_all_files(root);
        let thresholds = crate::config::Thresholds::default();
        let _metrics = analyze_functions(&symbols, &thresholds);
    }
}
