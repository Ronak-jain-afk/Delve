use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::parser::{Export, FileSymbols};

const EXT_PRIORITY: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"];

pub struct DepGraph {
    /// Maps file path → symbols extracted from that file
    pub all_symbols: HashMap<String, FileSymbols>,
    /// Maps (importer_file, imported_symbol) → Vec<(export_file, export_symbol)>
    pub import_edges: HashMap<(String, String), Vec<(String, String)>>,
    /// Reverse: maps (export_file, export_symbol) → Vec<(importer_file, imported_symbol)>
    pub reverse_imports: HashMap<(String, String), Vec<(String, String)>>,
    /// Entry points detected
    pub entry_points: Vec<String>,
    /// Exports that are reachable from entry points
    pub reachable_exports: HashSet<(String, String)>,
    /// Maps (re_exporter_file, re_exported_symbol) → Vec<(source_file, source_symbol)>
    /// For re-exports like `export { foo } from './bar'`
    pub re_export_edges: HashMap<(String, String), Vec<(String, String)>>,
}

impl DepGraph {
    pub fn new(symbols: Vec<FileSymbols>) -> Self {
        let mut all_symbols = HashMap::new();
        for s in symbols {
            let path = s.file_path.clone();
            all_symbols.insert(path, s);
        }
        DepGraph {
            all_symbols,
            import_edges: HashMap::new(),
            reverse_imports: HashMap::new(),
            entry_points: Vec::new(),
            reachable_exports: HashSet::new(),
            re_export_edges: HashMap::new(),
        }
    }

    pub fn build(&mut self) {
        let file_paths: Vec<String> = self.all_symbols.keys().cloned().collect();

        for file_path in &file_paths {
            if let Some(symbols) = self.all_symbols.get(file_path) {
                // Process regular imports
                for imp in &symbols.imports {
                    if let Some(resolved_path) = resolve_import(file_path, &imp.source) {
                        if let Some(target_symbols) = self.all_symbols.get(&resolved_path) {
                            for sym in &imp.symbols {
                                let has_matching_export = target_symbols
                                    .exports
                                    .iter()
                                    .any(|e| e.name == *sym);
                                if has_matching_export {
                                    let key = (file_path.clone(), sym.clone());
                                    self.import_edges
                                        .entry(key)
                                        .or_default()
                                        .push((resolved_path.clone(), sym.clone()));
                                    let rev_key = (resolved_path.clone(), sym.clone());
                                    self.reverse_imports
                                        .entry(rev_key)
                                        .or_default()
                                        .push((file_path.clone(), sym.clone()));
                                }
                            }
                            // If no specific symbols matched (namespace import), mark all exports
                            if imp.is_namespace {
                                for exp in &target_symbols.exports {
                                    let key = (file_path.clone(), exp.name.clone());
                                    self.import_edges
                                        .entry(key)
                                        .or_default()
                                        .push((resolved_path.clone(), exp.name.clone()));
                                    let rev_key = (resolved_path.clone(), exp.name.clone());
                                    self.reverse_imports
                                        .entry(rev_key)
                                        .or_default()
                                        .push((file_path.clone(), exp.name.clone()));
                                }
                            }
                        }
                    }
                }

                // Process named re-exports: export { foo } from './bar'
                for exp in &symbols.exports {
                    if let Some(ref source) = exp.re_export_source {
                        if let Some(resolved_source) = resolve_import(file_path, source) {
                            // Find the symbol in the source file
                            if let Some(source_sym) = self.all_symbols.get(&resolved_source) {
                                if source_sym.exports.iter().any(|e| e.name == exp.name) {
                                    let key = (file_path.clone(), exp.name.clone());
                                    self.re_export_edges
                                        .entry(key)
                                        .or_default()
                                        .push((resolved_source.clone(), exp.name.clone()));
                                }
                            }
                        }
                    }
                }

                // Process wildcard re-exports: export * from './bar'
                for wildcard in &symbols.wildcard_re_exports {
                    if let Some(resolved_source) = resolve_import(file_path, &wildcard.source) {
                        if let Some(source_sym) = self.all_symbols.get(&resolved_source) {
                            for src_exp in &source_sym.exports {
                                let key = (file_path.clone(), src_exp.name.clone());
                                self.re_export_edges
                                    .entry(key)
                                    .or_default()
                                    .push((resolved_source.clone(), src_exp.name.clone()));
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn detect_entry_points(&mut self) {
        let file_paths: Vec<String> = self.all_symbols.keys().cloned().collect();
        let mut candidates: HashSet<String> = HashSet::new();

        // Heuristic 1: package.json -> main, module, bin, exports
        if let Some(root) = find_project_root(&file_paths) {
            let pkg_path = root.join("package.json");
            if let Ok(content) = std::fs::read_to_string(&pkg_path) {
                if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                    for key in &["main", "module"] {
                        if let Some(val) = pkg.get(*key).and_then(|v| v.as_str()) {
                            let resolved = root.join(val);
                            if let Ok(canon) = resolved.canonicalize() {
                                let s = canon.to_string_lossy().to_string();
                                if self.all_symbols.contains_key(&s) {
                                    candidates.insert(s);
                                }
                            }
                        }
                    }
                    if let Some(bin) = pkg.get("bin") {
                        if let Some(path) = bin.as_str() {
                            let resolved = root.join(path);
                            if let Ok(canon) = resolved.canonicalize() {
                                let s = canon.to_string_lossy().to_string();
                                if self.all_symbols.contains_key(&s) {
                                    candidates.insert(s);
                                }
                            }
                        } else if let Some(obj) = bin.as_object() {
                            for (_, val) in obj {
                                if let Some(path) = val.as_str() {
                                    let resolved = root.join(path);
                                    if let Ok(canon) = resolved.canonicalize() {
                                        let s = canon.to_string_lossy().to_string();
                                        if self.all_symbols.contains_key(&s) {
                                            candidates.insert(s);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(exports) = pkg.get("exports") {
                        collect_export_paths(exports, &root, &mut candidates, &self.all_symbols);
                    }
                }
            }
        }

        // Heuristic 2: well-known filenames
        for path in &file_paths {
            let p = Path::new(path);
            if let Some(name) = p.file_stem().and_then(|s| s.to_str()) {
                match name {
                    "index" | "main" | "cli" | "app" => {
                        candidates.insert(path.clone());
                    }
                    _ => {}
                }
            }
        }

        // Heuristic 3: check for require.main === module or import.meta.url === ...
        for path in &file_paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                if content.contains("require.main === module")
                    || content.contains("import.meta.url")
                {
                    candidates.insert(path.clone());
                }
            }
        }

        self.entry_points = candidates.into_iter().collect();
    }

    pub fn traverse_from_entry_points(&mut self) {
        let mut visited_files: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();

        // Seed: all exports from entry points are reachable
        for ep in &self.entry_points {
            if let Some(symbols) = self.all_symbols.get(ep) {
                for exp in &symbols.exports {
                    self.reachable_exports
                        .insert((ep.clone(), exp.name.clone()));
                }
            }
            queue.push_back(ep.clone());
        }

        while let Some(file) = queue.pop_front() {
            if !visited_files.insert(file.clone()) {
                continue;
            }

            if let Some(symbols) = self.all_symbols.get(&file) {
                // Follow imports to find dependencies
                for imp in &symbols.imports {
                    if let Some(resolved) = resolve_import(&file, &imp.source) {
                        if self.all_symbols.contains_key(&resolved) {
                            for sym in &imp.symbols {
                                self.reachable_exports
                                    .insert((resolved.clone(), sym.clone()));
                            }
                            queue.push_back(resolved);
                        }
                    }
                }
            }

            // Follow re-export chains: if this file re-exports a symbol,
            // the original source symbol is also reachable
            let keys: Vec<_> = self
                .re_export_edges
                .keys()
                .filter(|(f, _)| *f == file)
                .cloned()
                .collect();
            for key in &keys {
                if self.reachable_exports.contains(key) {
                    if let Some(sources) = self.re_export_edges.get(key) {
                        for (src_file, src_sym) in sources {
                            self.reachable_exports
                                .insert((src_file.clone(), src_sym.clone()));
                            queue.push_back(src_file.clone());
                        }
                    }
                }
            }
        }
    }

    pub fn find_unused_exports(&self) -> Vec<(String, &Export)> {
        let mut unused = Vec::new();
        for (file_path, symbols) in &self.all_symbols {
            for exp in &symbols.exports {
                if !self.reachable_exports.contains(&(file_path.clone(), exp.name.clone())) {
                    unused.push((file_path.clone(), exp));
                }
            }
        }
        unused.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.start_line.cmp(&b.1.start_line)));
        unused
    }

    /// Find circular dependencies using Tarjan's SCC algorithm.
    /// Returns cycles where each cycle is a list of file paths.
    pub fn find_circular_dependencies(&self) -> Vec<Vec<String>> {
        // Build adjacency list: which files import from which
        let file_paths: Vec<String> = self.all_symbols.keys().cloned().collect();
        let file_set: HashSet<String> = file_paths.iter().cloned().collect();

        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for fp in &file_paths {
            adj.entry(fp.clone()).or_default();
        }
        for ((importer, _), targets) in &self.import_edges {
            for (target_file, _) in targets {
                if file_set.contains(importer) && file_set.contains(target_file) {
                    adj.entry(importer.clone())
                        .or_default()
                        .push(target_file.clone());
                }
            }
        }

        // Tarjan's SCC
        let mut index = 0usize;
        let mut stack: Vec<String> = Vec::new();
        let mut on_stack: HashSet<String> = HashSet::new();
        let mut indices: HashMap<String, usize> = HashMap::new();
        let mut lowlink: HashMap<String, usize> = HashMap::new();
        let mut sccs: Vec<Vec<String>> = Vec::new();

        fn strongconnect(
            v: &str,
            adj: &HashMap<String, Vec<String>>,
            index: &mut usize,
            stack: &mut Vec<String>,
            on_stack: &mut HashSet<String>,
            indices: &mut HashMap<String, usize>,
            lowlink: &mut HashMap<String, usize>,
            sccs: &mut Vec<Vec<String>>,
        ) {
            indices.insert(v.to_string(), *index);
            lowlink.insert(v.to_string(), *index);
            *index += 1;
            stack.push(v.to_string());
            on_stack.insert(v.to_string());

            if let Some(neighbors) = adj.get(v) {
                for w in neighbors {
                    if !indices.contains_key(w) {
                        strongconnect(w, adj, index, stack, on_stack, indices, lowlink, sccs);
                        let w_low = *lowlink.get(w).unwrap_or(&usize::MAX);
                        let v_low = *lowlink.get(v).unwrap_or(&usize::MAX);
                        lowlink.insert(v.to_string(), v_low.min(w_low));
                    } else if on_stack.contains(w) {
                        let w_idx = *indices.get(w).unwrap_or(&usize::MAX);
                        let v_low = *lowlink.get(v).unwrap_or(&usize::MAX);
                        lowlink.insert(v.to_string(), v_low.min(w_idx));
                    }
                }
            }

            if lowlink.get(v) == indices.get(v) {
                let mut scc: Vec<String> = Vec::new();
                loop {
                    let w = stack.pop().unwrap();
                    on_stack.remove(&w);
                    scc.push(w.clone());
                    if w == v {
                        break;
                    }
                }
                // Only keep SCCs with > 1 node (those are actual cycles)
                if scc.len() > 1 {
                    scc.sort();
                    sccs.push(scc);
                }
            }
        }

        let sorted: Vec<String> = file_paths.clone();
        for v in &sorted {
            if !indices.contains_key(v) {
                strongconnect(
                    v, &adj, &mut index, &mut stack, &mut on_stack,
                    &mut indices, &mut lowlink, &mut sccs,
                );
            }
        }

        sccs.sort_by(|a, b| a[0].cmp(&b[0]));
        sccs
    }

    /// Returns true if a file is a barrel file (all exports are re-exports).
    pub fn is_barrel_file(&self, file_path: &str) -> bool {
        if let Some(symbols) = self.all_symbols.get(file_path) {
            if symbols.exports.is_empty() {
                return false;
            }
            symbols.exports.iter().all(|e| e.re_export_source.is_some())
        } else {
            false
        }
    }
}

fn find_project_root(file_paths: &[String]) -> Option<PathBuf> {
    if file_paths.is_empty() {
        return None;
    }
    let sample = Path::new(&file_paths[0]);
    let parent = sample.parent()?;
    for ancestor in parent.ancestors() {
        let pkg = ancestor.join("package.json");
        if pkg.exists() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn collect_export_paths(
    value: &serde_json::Value,
    root: &Path,
    candidates: &mut HashSet<String>,
    all_symbols: &HashMap<String, FileSymbols>,
) {
    match value {
        serde_json::Value::String(s) => {
            let resolved = root.join(s);
            if let Ok(canon) = resolved.canonicalize() {
                let s = canon.to_string_lossy().to_string();
                if all_symbols.contains_key(&s) {
                    candidates.insert(s);
                }
            }
        }
        serde_json::Value::Object(map) => {
            for (_, val) in map {
                collect_export_paths(val, root, candidates, all_symbols);
            }
        }
        _ => {}
    }
}

pub fn resolve_import(current_file: &str, import_path: &str) -> Option<String> {
    let current = Path::new(current_file);
    let current_dir = current.parent()?;

    if import_path.starts_with('.') {
        let base = current_dir.join(import_path);

        for ext in EXT_PRIORITY {
            let candidate = base.with_extension(ext.trim_start_matches('.'));
            if candidate.exists() {
                return candidate
                    .canonicalize()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string());
            }
        }

        for ext in EXT_PRIORITY {
            let candidate = base
                .join("index")
                .with_extension(ext.trim_start_matches('.'));
            if candidate.exists() {
                return candidate
                    .canonicalize()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string());
            }
        }

        None
    } else {
        resolve_package_import(current_file, import_path)
    }
}

fn resolve_package_import(current_file: &str, import_path: &str) -> Option<String> {
    let current = Path::new(current_file);
    let mut dir = current.parent()?;

    let pkg_name = if import_path.starts_with('@') {
        let parts: Vec<&str> = import_path.splitn(3, '/').collect();
        if parts.len() >= 2 {
            format!("{}/{}", parts[0], parts[1])
        } else {
            return None;
        }
    } else {
        import_path.split('/').next()?.to_string()
    };

    loop {
        let nm = dir.join("node_modules").join(&pkg_name);
        let pkg_json = nm.join("package.json");

        if pkg_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(main) = pkg.get("main").and_then(|v| v.as_str()) {
                        let resolved = nm.join(main);
                        if resolved.exists() {
                            return resolved
                                .canonicalize()
                                .ok()
                                .map(|p| p.to_string_lossy().to_string());
                        }
                    }
                    let fallback = nm.join("index.js");
                    if fallback.exists() {
                        return fallback
                            .canonicalize()
                            .ok()
                            .map(|p| p.to_string_lossy().to_string());
                    }
                }
            }
        }

        if import_path.contains('/') && pkg_name != import_path {
            let sub_path = import_path
                .strip_prefix(&format!("{}/", pkg_name))
                .unwrap_or(import_path);
            let deep = nm.join(sub_path);
            for ext in EXT_PRIORITY {
                let candidate = deep.with_extension(ext.trim_start_matches('.'));
                if candidate.exists() {
                    return candidate
                        .canonicalize()
                        .ok()
                        .map(|p| p.to_string_lossy().to_string());
                }
            }
            let deep_index = deep.join("index.js");
            if deep_index.exists() {
                return deep_index
                    .canonicalize()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string());
            }
        }

        if let Some(parent) = dir.parent() {
            dir = parent;
        } else {
            break;
        }
    }

    None
}

pub fn build_complete_graph(root: &Path) -> DepGraph {
    build_complete_graph_with_ignore(root, &[])
}

pub fn build_complete_graph_with_ignore(root: &Path, ignore_patterns: &[String]) -> DepGraph {
    let symbols = crate::parser::parse_all_files_with_ignore(root, ignore_patterns);
    let mut graph = DepGraph::new(symbols);
    graph.build();
    graph.detect_entry_points();
    graph.traverse_from_entry_points();
    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_relative_import() {
        let current = std::fs::canonicalize("../../test-fixtures/vibe-app/src/index.ts")
            .unwrap()
            .to_string_lossy()
            .to_string();
        let resolved = resolve_import(&current, "./utils/formatDate");
        assert!(resolved.is_some(), "should resolve formatDate import");
        let resolved = resolved.unwrap();
        assert!(
            resolved.ends_with("formatDate.ts") || resolved.ends_with("formatDate.tsx"),
            "should resolve to formatDate.ts, got: {}",
            resolved
        );
    }

    #[test]
    fn test_vibe_app_graph() {
        let root = Path::new("../../test-fixtures/vibe-app");
        let graph = build_complete_graph(root);
        assert!(
            !graph.all_symbols.is_empty(),
            "should parse at least one file"
        );
    }

    #[test]
    fn test_entry_points_detected() {
        let root = Path::new("../../test-fixtures/vibe-app");
        let graph = build_complete_graph(root);
        assert!(
            !graph.entry_points.is_empty(),
            "should detect at least one entry point"
        );
        let has_index = graph
            .entry_points
            .iter()
            .any(|e| e.contains("index.ts"));
        assert!(
            has_index,
            "index.ts should be an entry point via package.json main field"
        );
    }

    #[test]
    fn test_reachable_exports() {
        let root = Path::new("../../test-fixtures/vibe-app");
        let graph = build_complete_graph(root);
        let has_main = graph
            .reachable_exports
            .iter()
            .any(|(f, n)| f.contains("index.ts") && n == "main");
        assert!(has_main, "main export in index.ts should be reachable");
    }

    #[test]
    fn test_unused_exports_detected() {
        let root = Path::new("../../test-fixtures/vibe-app");
        let graph = build_complete_graph(root);
        let unused = graph.find_unused_exports();
        let has_mouse_pos = unused
            .iter()
            .any(|(f, e)| f.contains("useScroll.ts") && e.name == "useMousePosition");
        assert!(
            has_mouse_pos,
            "useMousePosition should be detected as unused"
        );
    }

    #[test]
    fn test_definitely_used_not_in_unused() {
        let root = Path::new("../../test-fixtures/vibe-app");
        let graph = build_complete_graph(root);
        let unused = graph.find_unused_exports();
        let has_format_date = unused
            .iter()
            .any(|(f, e)| f.contains("formatDate.ts") && e.name == "formatDate");
        assert!(
            !has_format_date,
            "formatDate is imported and should not be unused"
        );
    }

    // --- Barrel file tests ---

    #[test]
    fn test_re_export_detected() {
        let root = Path::new("../../test-fixtures/v2");
        let graph = build_complete_graph(root);
        let barrel = graph.all_symbols.values().find(|s| s.file_path.ends_with("barrel.ts"));
        assert!(barrel.is_some(), "should parse barrel.ts");
        let barrel = barrel.unwrap();

        let re_exports: Vec<_> = barrel.exports.iter().filter(|e| e.re_export_source.is_some()).collect();
        assert!(!re_exports.is_empty(), "barrel.ts should have re-exports");
        assert!(re_exports.iter().any(|e| e.name == "helperA"), "should re-export helperA");
        assert!(re_exports.iter().any(|e| e.name == "helperB"), "should re-export helperB");
    }

    #[test]
    fn test_re_export_propagation() {
        // A.ts exports helperA, barrel.ts re-exports helperA from A.ts,
        // consumer.ts imports helperA from barrel.ts.
        // helperA in A.ts should be reachable.
        let root = Path::new("../../test-fixtures/v2");
        let graph = build_complete_graph(root);
        let has_helper_a = graph
            .reachable_exports
            .iter()
            .any(|(f, n)| f.contains("A.ts") && n == "helperA");
        assert!(
            has_helper_a,
            "helperA in A.ts should be reachable via barrel.ts re-export"
        );
    }

    #[test]
    fn test_barrel_file_detection() {
        let root = Path::new("../../test-fixtures/v2");
        let graph = build_complete_graph(root);
        let barrel_path = graph.all_symbols.keys().find(|p| p.contains("barrel.ts"));
        assert!(barrel_path.is_some(), "should find barrel.ts");
        assert!(
            graph.is_barrel_file(barrel_path.unwrap()),
            "barrel.ts should be detected as barrel file"
        );
    }

    #[test]
    fn test_wildcard_re_export_propagation() {
        let root = Path::new("../../test-fixtures/v2");
        let graph = build_complete_graph(root);
        let has_wildcard_export = graph
            .reachable_exports
            .iter()
            .any(|(f, n)| f.contains("wildcard_a.ts") && (n == "funcA" || n == "funcB"));
        assert!(
            has_wildcard_export,
            "wildcard re-exports should be reachable"
        );
    }

    // --- Type-only import tests ---

    #[test]
    fn test_type_only_import_detected() {
        let root = Path::new("../../test-fixtures/v2");
        let graph = build_complete_graph(root);
        let consumer = graph.all_symbols.values().find(|s| s.file_path.ends_with("type_consumer.ts"));
        assert!(consumer.is_some(), "should parse type_consumer.ts");
        let consumer = consumer.unwrap();

        let type_only_imports: Vec<_> = consumer.imports.iter().filter(|i| i.is_type_only).collect();
        assert!(!type_only_imports.is_empty(), "type_consumer.ts should have type-only imports");
    }

    // --- Circular dependency tests ---

    #[test]
    fn test_circular_dependency_detected() {
        let root = Path::new("../../test-fixtures/v2");
        let graph = build_complete_graph(root);
        let cycles = graph.find_circular_dependencies();
        let has_cycle = cycles.iter().any(|cycle| {
            cycle.iter().any(|f| f.contains("circ_a.ts"))
                && cycle.iter().any(|f| f.contains("circ_b.ts"))
        });
        assert!(has_cycle, "circ_a.ts and circ_b.ts should form a cycle");
    }

    #[test]
    fn test_no_false_cycle_on_acyclic() {
        // vibe-app should have no circular deps
        let root = Path::new("../../test-fixtures/vibe-app");
        let graph = build_complete_graph(root);
        let cycles = graph.find_circular_dependencies();
        assert!(
            cycles.is_empty(),
            "vibe-app should have no circular dependencies"
        );
    }
}
