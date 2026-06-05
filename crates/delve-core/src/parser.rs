use std::path::Path;
use std::sync::Mutex;

use ignore::WalkBuilder;
use tree_sitter::{Node, Parser, Tree};

static TS_PARSER: Mutex<Option<Parser>> = Mutex::new(None);
static JS_PARSER: Mutex<Option<Parser>> = Mutex::new(None);

fn with_parser<F, R>(ts: bool, f: F) -> R
where
    F: FnOnce(&mut Parser) -> R,
{
    let lock = if ts { &TS_PARSER } else { &JS_PARSER };
    let mut guard = lock.lock().unwrap();
    if guard.is_none() {
        let mut p = Parser::new();
        if ts {
            p.set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
                .expect("Failed to load TSX grammar");
        } else {
            p.set_language(&tree_sitter_javascript::LANGUAGE.into())
                .expect("Failed to load JavaScript grammar");
        }
        *guard = Some(p);
    }
    f(guard.as_mut().unwrap())
}

pub fn language_for_file(path: &str) -> &'static str {
    let p = path.to_lowercase();
    if p.ends_with(".tsx") {
        "tsx"
    } else if p.ends_with(".ts") || p.ends_with(".mts") || p.ends_with(".cts") {
        "ts"
    } else {
        "js"
    }
}

fn parse_file(path: &str, source: &str) -> Option<Tree> {
    let lang = language_for_file(path);
    match lang {
        "ts" | "tsx" => with_parser(true, |p| p.parse(source, None)),
        "js" => with_parser(false, |p| p.parse(source, None)),
        _ => None,
    }
}

pub fn find_source_files(root: &Path) -> Vec<String> {
    find_source_files_with_ignore(root, &[])
}

pub fn find_source_files_with_ignore(root: &Path, ignore_patterns: &[String]) -> Vec<String> {
    let mut files = Vec::new();
    let root = if root.is_relative() {
        std::env::current_dir()
            .unwrap_or_default()
            .join(root)
    } else {
        root.to_path_buf()
    };
    let walker = WalkBuilder::new(&root)
        .git_ignore(true)
        .standard_filters(true)
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_lowercase().as_str() {
                "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
                    let path_str = path.to_string_lossy();
                    if ignore_patterns.iter().any(|p| path_str.contains(p.as_str())) {
                        continue;
                    }
                    if let Ok(canonical) = path.canonicalize() {
                        files.push(canonical.to_string_lossy().to_string());
                    } else {
                        files.push(path_str.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    files
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExportKind {
    Function,
    Const,
    Class,
    Interface,
    Type,
    Default,
    Named,
}

#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub start_line: usize,
    pub end_line: usize,
    pub file_path: String,
    pub is_used: bool,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub symbols: Vec<String>,
    pub source: String,
    pub start_line: usize,
    pub end_line: usize,
    pub file_path: String,
    pub is_default: bool,
    pub is_namespace: bool,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub body_start_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub file_path: String,
}

#[derive(Debug, Clone)]
pub struct FileSymbols {
    pub file_path: String,
    pub exports: Vec<Export>,
    pub imports: Vec<Import>,
    pub functions: Vec<FunctionInfo>,
}

fn node_text<'a>(source: &'a str, node: Node) -> &'a str {
    &source[node.byte_range()]
}

fn make_function_info(node: Node, name: Option<String>, file_path: &str) -> FunctionInfo {
    FunctionInfo {
        name,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        body_start_line: node.start_position().row + 1,
        start_byte: node.byte_range().start,
        end_byte: node.byte_range().end,
        file_path: file_path.to_string(),
    }
}

fn collect_functions(node: Node, source: &str, file_path: &str, functions: &mut Vec<FunctionInfo>) {
    let kind = node.kind();
    if kind == "function_declaration"
        || kind == "generator_function_declaration"
    {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(source, n).to_string());
        functions.push(make_function_info(node, name, file_path));
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(child, source, file_path, functions);
    }
}

fn collect_function_expressions(
    node: Node,
    source: &str,
    file_path: &str,
    functions: &mut Vec<FunctionInfo>,
) {
    let kind = node.kind();
    if kind == "function" || kind == "arrow_function" {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(source, n).to_string());
        functions.push(make_function_info(node, name, file_path));
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_function_expressions(child, source, file_path, functions);
    }
}

fn collect_exports(
    node: Node,
    source: &str,
    file_path: &str,
    exports: &mut Vec<Export>,
    functions: &mut Vec<FunctionInfo>,
) {
    match node.kind() {
        "export_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "function_declaration" | "generator_function_declaration" => {
                        if let Some(name) = child
                            .child_by_field_name("name")
                            .map(|n| node_text(source, n))
                        {
                            let start = child.start_position().row + 1;
                            let end = child.end_position().row + 1;
                            exports.push(Export {
                                name: name.to_string(),
                                kind: ExportKind::Function,
                                start_line: start,
                                end_line: end,
                                file_path: file_path.to_string(),
                                is_used: false,
                            });
                            functions.push(make_function_info(child, Some(name.to_string()), file_path));
                        }
                    }
                    "class_declaration" => {
                        if let Some(name) = child
                            .child_by_field_name("name")
                            .map(|n| node_text(source, n))
                        {
                            exports.push(Export {
                                name: name.to_string(),
                                kind: ExportKind::Class,
                                start_line: child.start_position().row + 1,
                                end_line: child.end_position().row + 1,
                                file_path: file_path.to_string(),
                                is_used: false,
                            });
                        }
                    }
                    "lexical_declaration" | "variable_declaration" => {
                        extract_variable_exports(child, source, file_path, exports, functions);
                    }
                    "export_clause" => {
                        let mut ec = child.walk();
                        for spec in child.children(&mut ec) {
                            if spec.kind() == "export_specifier" {
                                if let Some(name) = spec
                                    .child_by_field_name("name")
                                    .map(|n| node_text(source, n))
                                {
                                    exports.push(Export {
                                        name: name.to_string(),
                                        kind: ExportKind::Named,
                                        start_line: spec.start_position().row + 1,
                                        end_line: spec.end_position().row + 1,
                                        file_path: file_path.to_string(),
                                        is_used: false,
                                    });
                                }
                            }
                        }
                    }
                    "interface_declaration" => {
                        if let Some(name) = child
                            .child_by_field_name("name")
                            .map(|n| node_text(source, n))
                        {
                            exports.push(Export {
                                name: name.to_string(),
                                kind: ExportKind::Interface,
                                start_line: child.start_position().row + 1,
                                end_line: child.end_position().row + 1,
                                file_path: file_path.to_string(),
                                is_used: false,
                            });
                        }
                    }
                    "type_alias_declaration" => {
                        if let Some(name) = child
                            .child_by_field_name("name")
                            .map(|n| node_text(source, n))
                        {
                            exports.push(Export {
                                name: name.to_string(),
                                kind: ExportKind::Type,
                                start_line: child.start_position().row + 1,
                                end_line: child.end_position().row + 1,
                                file_path: file_path.to_string(),
                                is_used: false,
                            });
                        }
                    }
                    "assignment_expression" => {
                        if let Some(left) = child.child_by_field_name("left") {
                            let left_text = node_text(source, left);
                            if left_text == "module.exports" {
                                exports.push(Export {
                                    name: "default".to_string(),
                                    kind: ExportKind::Default,
                                    start_line: child.start_position().row + 1,
                                    end_line: child.end_position().row + 1,
                                    file_path: file_path.to_string(),
                                    is_used: false,
                                });
                            } else if let Some(prop) = left_text.strip_prefix("exports.") {
                                exports.push(Export {
                                    name: prop.to_string(),
                                    kind: ExportKind::Named,
                                    start_line: child.start_position().row + 1,
                                    end_line: child.end_position().row + 1,
                                    file_path: file_path.to_string(),
                                    is_used: false,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "expression_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "assignment_expression" {
                    if let Some(left) = child.child_by_field_name("left") {
                        let left_text = node_text(source, left);
                        if left_text == "module.exports" {
                            exports.push(Export {
                                name: "default".to_string(),
                                kind: ExportKind::Default,
                                start_line: child.start_position().row + 1,
                                end_line: child.end_position().row + 1,
                                file_path: file_path.to_string(),
                                is_used: false,
                            });
                        } else if let Some(prop) = left_text.strip_prefix("exports.") {
                            exports.push(Export {
                                name: prop.to_string(),
                                kind: ExportKind::Named,
                                start_line: child.start_position().row + 1,
                                end_line: child.end_position().row + 1,
                                file_path: file_path.to_string(),
                                is_used: false,
                            });
                        }
                    }
                }
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            collect_function_expressions(node, source, file_path, functions);
        }
        "function_declaration" | "generator_function_declaration"
        | "class_declaration" | "interface_declaration"
        | "type_alias_declaration" => {
            collect_functions(node, source, file_path, functions);
        }
        _ => {}
    }
}

fn extract_variable_exports(
    node: Node,
    source: &str,
    file_path: &str,
    exports: &mut Vec<Export>,
    functions: &mut Vec<FunctionInfo>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(source, name_node);
                let start = child.start_position().row + 1;
                let end = child.end_position().row + 1;
                exports.push(Export {
                    name: name.to_string(),
                    kind: ExportKind::Const,
                    start_line: start,
                    end_line: end,
                    file_path: file_path.to_string(),
                    is_used: false,
                });
                if let Some(value) = child.child_by_field_name("value") {
                    if value.kind() == "function" || value.kind() == "arrow_function" {
                        functions.push(make_function_info(value, Some(name.to_string()), file_path));
                    }
                }
            }
        }
    }
}

fn is_require_call(node: Node, source: &str) -> Option<String> {
    if node.kind() != "call_expression" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if node_text(source, func) != "require" {
        return None;
    }
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for arg in args.children(&mut cursor) {
        if arg.kind() == "string" || arg.kind() == "string_fragment" {
            let raw = node_text(source, arg);
            return Some(raw.trim_matches('\'').trim_matches('"').to_string());
        }
    }
    None
}

fn collect_imports(node: Node, source: &str, file_path: &str, imports: &mut Vec<Import>) {
    // ES import statements
    if node.kind() == "import_statement" {
        let mut symbols = Vec::new();
        let mut source_module = String::new();
        let mut is_default = false;
        let mut is_namespace = false;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_clause" => {
                    let mut ic = child.walk();
                    for c in child.children(&mut ic) {
                        match c.kind() {
                            "identifier" => {
                                symbols.push(node_text(source, c).to_string());
                                is_default = true;
                            }
                            "named_imports" => {
                                let mut ni = c.walk();
                                for spec in c.children(&mut ni) {
                                    if spec.kind() == "import_specifier" {
                                        if let Some(name) = spec
                                            .child_by_field_name("name")
                                            .map(|n| node_text(source, n))
                                        {
                                            symbols.push(name.to_string());
                                        }
                                    }
                                }
                            }
                            "namespace_import" => {
                                if let Some(ns) = c.child_by_field_name("name") {
                                    symbols.push(node_text(source, ns).to_string());
                                    is_namespace = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "string" | "string_fragment" => {
                    let raw = node_text(source, child);
                    source_module = raw.trim_matches('\'').trim_matches('"').to_string();
                }
                _ => {}
            }
        }

        if !symbols.is_empty() && !source_module.is_empty() {
            imports.push(Import {
                symbols,
                source: source_module,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                file_path: file_path.to_string(),
                is_default,
                is_namespace,
            });
        }
        return;
    }

    // CommonJS require() calls: const foo = require('./bar')
    if node.kind() == "variable_declarator" {
        if let Some(value) = node.child_by_field_name("value") {
            if let Some(source_module) = is_require_call(value, source) {
                let symbol = node
                    .child_by_field_name("name")
                    .map(|n| node_text(source, n).to_string())
                    .unwrap_or_else(|| "default".to_string());
                imports.push(Import {
                    symbols: vec![symbol],
                    source: source_module,
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    file_path: file_path.to_string(),
                    is_default: true,
                    is_namespace: false,
                });
                return;
            }
        }
    }

    // Standalone require('./foo') calls
    if node.kind() == "expression_statement" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(source_module) = is_require_call(child, source) {
                imports.push(Import {
                    symbols: vec!["default".to_string()],
                    source: source_module,
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    file_path: file_path.to_string(),
                    is_default: true,
                    is_namespace: false,
                });
                return;
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_imports(child, source, file_path, imports);
    }
}

pub fn extract_file_symbols(file_path: &str, source: &str) -> Option<FileSymbols> {
    let tree = parse_file(file_path, source)?;
    let root = tree.root_node();

    let mut exports = Vec::new();
    let mut imports = Vec::new();
    let mut functions = Vec::new();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        collect_exports(child, source, file_path, &mut exports, &mut functions);
        collect_imports(child, source, file_path, &mut imports);
    }

    Some(FileSymbols {
        file_path: file_path.to_string(),
        exports,
        imports,
        functions,
    })
}

use rayon::prelude::*;

pub fn parse_all_files(root: &Path) -> Vec<FileSymbols> {
    parse_all_files_with_ignore(root, &[])
}

pub fn parse_all_files_with_ignore(root: &Path, ignore_patterns: &[String]) -> Vec<FileSymbols> {
    let files = find_source_files_with_ignore(root, ignore_patterns);
    files
        .par_iter()
        .filter_map(|file_path| {
            let source = std::fs::read_to_string(file_path).ok()?;
            extract_file_symbols(file_path, &source)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_source_files() {
        let test_dir = Path::new("../../test-fixtures/vibe-app");
        let files = find_source_files(test_dir);
        assert!(!files.is_empty(), "should find source files");
        assert!(
            files.iter().any(|f| f.ends_with(".ts") || f.ends_with(".tsx")),
            "should find ts/tsx files"
        );
    }

    #[test]
    fn test_parse_index_ts() {
        let path = "../../test-fixtures/vibe-app/src/index.ts";
        let source = std::fs::read_to_string(path).unwrap();
        let symbols = extract_file_symbols(path, &source).unwrap();
        assert_eq!(symbols.exports.len(), 1);
        assert_eq!(symbols.exports[0].name, "main");
        assert_eq!(symbols.imports.len(), 2);
    }

    #[test]
    fn test_parse_format_date() {
        let path = "../../test-fixtures/vibe-app/src/utils/formatDate.ts";
        let source = std::fs::read_to_string(path).unwrap();
        let symbols = extract_file_symbols(path, &source).unwrap();
        let names: Vec<_> = symbols.exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"formatTimestamp"));
        assert!(names.contains(&"formatDate"));
    }

    #[test]
    fn test_parse_dashboard() {
        let path = "../../test-fixtures/vibe-app/src/components/Dashboard.tsx";
        let source = std::fs::read_to_string(path).unwrap();
        let symbols = extract_file_symbols(path, &source).unwrap();
        let export_names: Vec<_> = symbols.exports.iter().map(|e| e.name.as_str()).collect();
        assert!(export_names.contains(&"renderDashboard"));
        assert!(symbols.functions.len() >= 3);
    }
}
